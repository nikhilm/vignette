extern crate libc;
extern crate nix;
extern crate unwind_sys;

use self::{
    nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal},
    unwind_sys::*,
};
use std::{cell::UnsafeCell, fs, io, mem, process};

// not-so-opaque wrapper around pid_t
pub type ThreadId = libc::pid_t;

fn gettid() -> libc::pid_t {
    unsafe { libc::syscall(libc::SYS_gettid) as libc::pid_t }
}

pub fn get_current_thread() -> ThreadId {
    gettid()
}

pub fn is_current_thread(id: &ThreadId) -> bool {
    gettid() == *id
}

/// wraps a POSIX semaphore
///
/// We need to use these as only sem_post is required to be signal safe.
struct PosixSemaphore {
    sem: UnsafeCell<libc::sem_t>,
}

impl PosixSemaphore {
    /// Returns a new semaphore if initialization succeeded.
    ///
    /// TODO: Consider exposing error code.
    pub fn new(value: u32) -> io::Result<Self> {
        let mut sem: libc::sem_t = unsafe { mem::uninitialized() };
        let r = unsafe {
            libc::sem_init(&mut sem, 0 /* not shared */, value)
        };
        if r == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(PosixSemaphore {
            sem: UnsafeCell::new(sem),
        })
    }

    pub fn post(&self) -> io::Result<()> {
        if unsafe { libc::sem_post(self.sem.get()) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub fn wait(&self) -> io::Result<()> {
        if unsafe { libc::sem_wait(self.sem.get()) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    /// Retries the wait if it returned due to EINTR.
    ///
    /// Returns Ok on success and the error on any other return value.
    pub fn wait_through_intr(&self) -> io::Result<()> {
        loop {
            match self.wait() {
                Err(os_error) => {
                    let err = os_error.raw_os_error().expect("os error");
                    if err == libc::EINTR {
                        continue;
                    }
                    return Err(os_error);
                }
                _ => return Ok(()),
            }
        }
    }
}

unsafe impl Sync for PosixSemaphore {}

impl Drop for PosixSemaphore {
    /// Destroys the semaphore.
    fn drop(&mut self) {
        unsafe { libc::sem_destroy(self.sem.get()) };
    }
}

// TODO: Add a test for ensuring we don't sample over capacity.
// Then we can look at storing module info and post-facto symbolication using either debug files or sym files.
// options are
// 1. use the breakpad-symbols crate + dump_syms.
// 2. use goblin over the unstripped binaries.

/// Iterates over task threads by reading /proc.
///
/// IMPORTANT: This iterator also returns the sampling thread!
pub fn thread_iterator() -> io::Result<impl Iterator<Item = io::Result<ThreadId>>> {
    fs::read_dir("/proc/self/task").map(|r| {
        r.map(|entry| {
            entry.map(|dir_entry| {
                let file = dir_entry.file_name().into_string().expect("valid utf8");
                file.parse::<libc::pid_t>().expect("tid should be pid_t")
            })
        })
    })
}

struct SharedState {
    // "msg1" is the signal.
    msg2: Option<PosixSemaphore>,
    msg3: Option<PosixSemaphore>,
    msg4: Option<PosixSemaphore>,
    context: Option<libc::ucontext_t>,
}

// TODO: Think about how we can use some rust typisms to make this cleaner.
// DO NOT use this from multiple threads at the same time.
static mut SHARED_STATE: SharedState = SharedState {
    msg2: None,
    msg3: None,
    msg4: None,
    context: None,
};

fn clear_shared_state() {
    unsafe {
        SHARED_STATE.msg2 = None;
        SHARED_STATE.msg3 = None;
        SHARED_STATE.msg4 = None;
        SHARED_STATE.context = None;
    }
}

fn reset_shared_state() {
    unsafe {
        SHARED_STATE.msg2 = Some(PosixSemaphore::new(0).expect("valid semaphore"));
        SHARED_STATE.msg3 = Some(PosixSemaphore::new(0).expect("valid semaphore"));
        SHARED_STATE.msg4 = Some(PosixSemaphore::new(0).expect("valid semaphore"));
        SHARED_STATE.context = None;
    }
}

/// Set's up the SIGPROF handler.
///
/// Dropping this reset's the handler.
pub struct Sampler {
    old_handler: SigAction,
}

impl Sampler {
    pub fn new() -> Self {
        let handler = SigHandler::SigAction(sigprof_handler);
        let action = SigAction::new(
            handler,
            SaFlags::SA_RESTART | SaFlags::SA_SIGINFO,
            SigSet::empty(),
        );
        let old = unsafe { sigaction(Signal::SIGPROF, &action).expect("signal handler set") };

        Sampler { old_handler: old }
    }

    /// Calls the callback with a suspended thread, then resumes the thread.
    ///
    /// This function is dangerous!
    /// 1. This function is not safe to call from multiple threads at the same time, nor is it safe
    ///    to create multiple instances of Sampler and call this on two of them concurrently as it
    ///    relies on global shared state.
    /// 2. Callback must not perform any heap allocations, nor must it interact with any other
    ///    shared locks that sampled threads can access.
    /// 3. Callback should return as quickly as possible to keep the program performant.
    pub fn suspend_and_resume_thread<F, T>(&self, thread: ThreadId, callback: F) -> T
    where
        F: FnOnce(&mut libc::ucontext_t) -> T,
    {
        debug_assert!(!is_current_thread(&thread), "Can't suspend sampler itself!");

        // first we reinitialize the semaphores
        reset_shared_state();

        // signal the thread, wait for it to tell us state was copied.
        send_sigprof(thread);
        unsafe {
            SHARED_STATE
                .msg2
                .as_ref()
                .unwrap()
                .wait_through_intr()
                .expect("msg2 wait succeeded");
        }

        let results = unsafe { callback(&mut SHARED_STATE.context.expect("valid context")) };

        // signal the thread to continue.
        unsafe {
            SHARED_STATE.msg3.as_ref().unwrap().post().expect("posted");
        }

        // wait for thread to continue.
        unsafe {
            SHARED_STATE
                .msg4
                .as_ref()
                .unwrap()
                .wait_through_intr()
                .expect("msg4 wait succeeded");
        }

        clear_shared_state();
        results
    }
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        unsafe {
            sigaction(Signal::SIGPROF, &self.old_handler).expect("previous signal handler restored")
        };
    }
}

extern "C" fn sigprof_handler(
    sig: libc::c_int,
    _info: *mut libc::siginfo_t,
    ctx: *mut libc::c_void,
) {
    assert_eq!(sig, libc::SIGPROF);
    unsafe {
        // copy the context.
        let context: libc::ucontext_t = *(ctx as *mut libc::ucontext_t);
        SHARED_STATE.context = Some(context);
        // Tell the sampler we copied the context.
        SHARED_STATE.msg2.as_ref().unwrap().post().expect("posted");

        // Wait for sampling to finish.
        SHARED_STATE
            .msg3
            .as_ref()
            .unwrap()
            .wait_through_intr()
            .expect("msg3 wait succeeded");

        // OK we are done!
        SHARED_STATE.msg4.as_ref().unwrap().post().expect("posted");
        // DO NOT TOUCH shared state here onwards.
    }
}

/// `to` is a Linux task ID.
fn send_sigprof(to: libc::pid_t) {
    unsafe {
        libc::syscall(libc::SYS_tgkill, process::id(), to, libc::SIGPROF);
    }
}

/// This definition will evolve as we go along.
#[derive(Debug, Hash)]
pub struct Frame {
    #[cfg(target_pointer_width = "32")]
    pub ip: u32,
    #[cfg(target_pointer_width = "64")]
    pub ip: u64,
}

/// Support for collecting one sample.
///
/// A sample should be created, then passed the context to unwind and finally one can retrieve the
/// frames.
///
/// Creation should be done outside the suspend_and_resume_thread call!
pub struct Sample {
    frames: Vec<Frame>,
}

// TODO: We probably don't want the actual collection to happen in a publicly exposed struct that
// is meant for representation.
impl Sample {
    /// Creates a new Sample.
    ///
    /// This sample will hold upto max_frames frames.
    /// The collection begins from the bottom-most function on the stack, so once the limit is
    /// reached, top frames are dropped.
    ///
    /// This is NOT safe to use within suspend_and_resume_thread.
    pub fn new(max_frames: usize) -> Self {
        Sample {
            frames: Vec::with_capacity(max_frames),
        }
    }

    // TODO: Use failure for better errors + wrap unwind errors.
    /// Unwind a stack from a context.
    ///
    /// Returns the collected frames.
    /// The length of the vector is the actual collected frames (<= max_frames).
    ///
    /// This IS safe to use within suspend_and_resume_thread.
    ///
    /// TODO: Right now if stepping fails, this whole function fails, but we may want to return the
    /// frames we have. We also probably want another state to indicate we had more frames than
    /// capacity, so users can report some kind of stats.
    pub fn collect(mut self, context: &mut libc::ucontext_t) -> Result<Vec<Frame>, i32> {
        // This is a stack allocation, so it is OK.
        let mut cursor: unw_cursor_t = unsafe { mem::uninitialized() };

        // A unw_context_t is an alias to the ucontext_t as clarified by the docs, so we can
        // use the signal context.
        let init = unsafe { unw_init_local(&mut cursor, context) };
        if init < 0 {
            return Err(init);
        }
        loop {
            if self.frames.len() == self.frames.capacity() {
                break;
            }
            let step = unsafe { unw_step(&mut cursor) };
            if step == 0 {
                // No more frames.
                break;
            } else if step < 0 {
                return Err(step);
            }

            let mut ip = 0;
            let rr = unsafe { unw_get_reg(&mut cursor, UNW_REG_IP, &mut ip) };
            if rr < 0 {
                return Err(rr);
            }
            // Move semantics OK as there is no allocation.
            let frame = Frame { ip };
            self.frames.push(frame);
        }

        Ok(self.frames)
    }
}

/// TODO: Next step is to add criterion based benchmarks.

// WARNING WARNING WARNING WARNING WARNING
//
// These tests MUST be run sequentially (`cargo test -- --test-threads 1`) as they install signal
// handlers that are process-wide!
#[cfg(test)]
mod tests {
    extern crate libc;
    extern crate nix;
    extern crate rustc_demangle;
    extern crate std;

    use super::*;

    use self::rustc_demangle::demangle;
    use std::{
        sync::{mpsc::channel, Arc},
        thread::spawn,
    };

    static mut SIGNAL_RECEIVED: bool = false;

    extern "C" fn acknowledge_sigprof(
        sig: libc::c_int,
        _info: *mut libc::siginfo_t,
        _ctx: *mut libc::c_void,
    ) {
        assert_eq!(sig, libc::SIGPROF);
        unsafe {
            SIGNAL_RECEIVED = true;
        }
    }

    #[test]
    fn test_sigprof() {
        let handler = SigHandler::SigAction(acknowledge_sigprof);
        let action = SigAction::new(
            handler,
            SaFlags::SA_RESTART | SaFlags::SA_SIGINFO,
            SigSet::empty(),
        );
        unsafe {
            sigaction(Signal::SIGPROF, &action).expect("signal handler set");
        }

        let (tx, rx) = channel();
        // Just to get the thread to wait until the signal is sent.
        let (tx2, rx2) = channel();
        let handle = spawn(move || {
            let tid = unsafe { libc::syscall(libc::SYS_gettid) as libc::pid_t };
            tx.send(tid).unwrap();
            rx2.recv().unwrap();
        });

        let to = rx.recv().unwrap();
        send_sigprof(to);
        tx2.send(()).unwrap();
        handle.join().expect("successful join");
        unsafe {
            assert!(SIGNAL_RECEIVED);
        }
    }

    #[test]
    fn test_semaphore() {
        let semaphore = Arc::new(PosixSemaphore::new(0).expect("init"));
        let semaphoret = semaphore.clone();

        let handle = spawn(move || {
            semaphoret.post().expect("posted");
        });

        semaphore.wait().expect("received wait");
        handle.join().expect("successful join");
    }

    #[test]
    fn test_thread_iterator() {
        let tasks: Vec<libc::pid_t> = thread_iterator()
            .expect("threads")
            .map(|x| x.expect("tid listed").0)
            .collect();
        assert!(tasks.contains(&gettid()));
    }

    #[test]
    fn test_suspend_resume() {
        let sampler = Sampler::new();
        let (tx, rx) = channel();
        // Just to get the thread to wait until the test is done.
        let (tx2, rx2) = channel();
        let handle = spawn(move || {
            tx.send(ThreadId(gettid())).unwrap();
            rx2.recv().unwrap();
        });

        let to = rx.recv().unwrap();
        sampler.suspend_and_resume_thread(&to, |context| {
            // TODO: This is where we would want to use libunwind in a real program.
            assert!(context.uc_stack.ss_size > 0);

            // we can tell the thread to shutdown once it is resumed.
            tx2.send(()).unwrap();
        });

        handle.join().unwrap();
        // make sure we cleaned up.
        unsafe {
            assert!(SHARED_STATE.context.is_none());
        }
    }

    #[test]
    #[should_panic]
    fn test_suspend_resume_itself() {
        let sampler = Sampler::new();
        let to = get_current_thread();
        sampler.suspend_and_resume_thread(&to, |_| {});
    }

    #[test]
    #[ignore] // Useful for playing around, but not required.
    fn test_suspend_resume_unwind() {
        let sampler = Sampler::new();
        let (tx, rx) = channel();
        // Just to get the thread to wait until the test is done.
        let (tx2, rx2) = channel();
        let handle = spawn(move || {
            let baz = || {
                tx.send(ThreadId(gettid())).unwrap();
                rx2.recv().unwrap();
            };
            let bar = || {
                baz();
            };

            let foo = || {
                bar();
            };

            foo();
        });

        let to = rx.recv().unwrap();
        sampler.suspend_and_resume_thread(&to, |context| unsafe {
            // TODO: This is where we would want to use libunwind in a real program.
            assert!(context.uc_stack.ss_size > 0);

            let mut cursor: unw_cursor_t = mem::uninitialized();
            let mut offset = 0;
            // A unw_context_t is an alias to the ucontext_t as clarified by the docs, so we can
            // use the signal context.
            unw_init_local(&mut cursor, context);
            while unw_step(&mut cursor) > 0 {
                let mut buf = vec![0; 256];
                // This won't actually work in non-debug info ELFs.
                // Plus it hurts timing.
                let r = unw_get_proc_name(
                    &mut cursor,
                    buf.as_mut_ptr() as *mut i8,
                    buf.len(),
                    &mut offset,
                );
                if r < 0 {
                    eprintln!("error {}", r);
                } else {
                    let len = buf.iter().position(|b| *b == 0).unwrap();
                    buf.truncate(len);
                    let name = String::from_utf8_lossy(&buf).into_owned();
                    eprintln!("fn {:#}", demangle(&name));
                }
            }

            // we can tell the thread to shutdown once it is resumed.
            tx2.send(()).unwrap();
        });

        handle.join().unwrap();
        // make sure we cleaned up.
        unsafe {
            assert!(SHARED_STATE.context.is_none());
        }
    }
}
