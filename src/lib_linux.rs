extern crate libc;
extern crate nix;

use self::libc::{pthread_kill, SIGPROF};
use self::nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet};
use std::cell::UnsafeCell;
use std::mem;
use std::os::unix::thread::JoinHandleExt;
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

fn send_sigprof(to_kill: &JoinHandleExt) {
    unsafe {
        libc::pthread_kill(to_kill.as_pthread_t(), libc::SIGPROF);
    }
}

#[cfg(test)]
mod tests {
    extern crate libc;
    extern crate nix;
    extern crate std;

    use super::*;

    use self::nix::sys::signal::{sigaction, Signal};

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

        let barrier = Arc::new(Barrier::new(2));
        let barriert = barrier.clone();

        let handle = spawn(move || {
            barriert.wait();
        });

        send_sigprof(&handle);
        barrier.wait();
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
}
