extern crate libc;
extern crate nix;

use self::libc::{pthread_kill, SIGPROF};
use self::nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet};
use std::cell::UnsafeCell;
use std::error;
use std::fs;
use std::io;
use std::iter;
use std::mem;
use std::os::unix::thread::JoinHandleExt;
use std::process;
use std::sync::Arc;
use std::sync::Barrier;
use std::thread::spawn;
use std::thread::Thread;

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
    pub fn new(value: u32) -> Option<PosixSemaphore> {
        let mut sem: libc::sem_t = unsafe { mem::uninitialized() };
        let r = unsafe {
            libc::sem_init(&mut sem, 0 /* not shared */, value)
        };
        if r == -1 {
            return None;
        }
        Some(PosixSemaphore {
            sem: UnsafeCell::new(sem),
        })
    }

    pub fn post(&self) -> Option<()> {
        unsafe { libc::sem_post(self.sem.get()) };
        Some(())
    }

    pub fn wait(&self) -> Option<()> {
        unsafe { libc::sem_wait(self.sem.get()) };
        Some(())
    }
}

unsafe impl Sync for PosixSemaphore {}

impl Drop for PosixSemaphore {
    fn drop(&mut self) {
        unsafe { libc::sem_destroy(self.sem.get()) };
    }
}

struct SharedState {
    barrier1: Barrier,
    context: *mut libc::ucontext_t,
}

/// Iterates over task threads by reading /proc.
pub fn thread_iterator() -> io::Result<impl Iterator<Item = io::Result<libc::pid_t>>> {
    fs::read_dir("/proc/self/task").map(|r| {
        r.map(|entry| {
            entry.map(|dir_entry| {
                let file = dir_entry.file_name().into_string().expect("valid utf8");
                file.parse::<libc::pid_t>().expect("tid should be pid_t")
            })
        })
    })
}

/// `to` is a Linux task ID.
fn send_sigprof(to: libc::pid_t) {
    unsafe {
        libc::syscall(libc::SYS_tgkill, process::id(), to, libc::SIGPROF);
    }
}

#[cfg(test)]
mod tests {
    extern crate libc;
    extern crate nix;
    extern crate std;

    use super::*;

    use self::nix::sys::signal::{sigaction, Signal};
    use std::sync::mpsc::channel;

    static mut signal_received: bool = false;

    extern "C" fn acknowledge_sigprof(
        sig: libc::c_int,
        info: *mut libc::siginfo_t,
        ctx: *mut libc::c_void,
    ) {
        assert_eq!(sig, libc::SIGPROF);
        unsafe {
            signal_received = true;
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
            assert!(signal_received);
        }
    }

    #[test]
    fn test_semaphore() {
        let semaphore = Arc::new(PosixSemaphore::new(0).expect("init"));
        let semaphoret = semaphore.clone();

        let handle = spawn(move || {
            semaphoret.post();
        });

        semaphore.wait();
        handle.join().expect("successful join");
    }

    #[test]
    fn test_thread_iterator() {
        let tid = unsafe { libc::syscall(libc::SYS_gettid) as libc::pid_t };
        let tasks: Vec<libc::pid_t> = thread_iterator()
            .expect("threads")
            .map(|x| x.expect("tid listed"))
            .collect();
        assert!(tasks.contains(&tid));
    }
}
