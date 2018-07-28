extern crate libc;
extern crate nix;

use self::libc::{pthread_kill, SIGPROF};
use self::nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet};
use std::os::unix::thread::JoinHandleExt;
use std::sync::Arc;
use std::sync::Barrier;
use std::thread::spawn;
use std::thread::Thread;

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
}
