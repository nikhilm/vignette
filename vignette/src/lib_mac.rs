#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

extern crate mach;
extern crate threadinfo;

mod unw {
    include!(concat!(env!("OUT_DIR"), "/unwind_bindings.rs"));
}

use std::{cell::UnsafeCell, fs, io, mem, process};

use self::mach::{
    kern_return::KERN_SUCCESS,
    message::mach_msg_type_number_t,
    structs::x86_thread_state64_t,
    thread_act::thread_get_state,
    thread_status::{thread_state_t, x86_THREAD_STATE64},
};

use self::threadinfo::{current_thread, Thread};
use types::{Frame, Sample, Unwinder};

pub struct Sampler {}

impl Sampler {
    pub fn new() -> Self {
        Sampler {}
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
    // TODO: Need to reconcile context passing between platforms
    // And unwinders, so we possibly need to pull this back into the platform specific files
    pub fn suspend_and_resume_thread<F, T>(&self, thread: Thread, callback: F) -> T
    where
        F: FnOnce(&mut unw::unw_context_t) -> T,
    {
        debug_assert!(!thread.is_current_thread(), "Can't suspend sampler itself!");

        thread.suspend().unwrap();
        let mut count: mach_msg_type_number_t = x86_thread_state64_t::count();
        let mut thread_state: x86_thread_state64_t = unsafe { std::mem::uninitialized() };
        let mut thread_state_ptr: thread_state_t = &mut thread_state as *mut _ as thread_state_t;
        let r =
            unsafe { thread_get_state(thread.0, x86_THREAD_STATE64, thread_state_ptr, &mut count) };
        assert!(r == KERN_SUCCESS, format!("{}", r));
        assert!(
            std::mem::size_of::<unw::unw_context_t>()
                >= std::mem::size_of::<x86_thread_state64_t>()
        );
        let mut context: unw::unw_context_t = unsafe { std::mem::zeroed() };
        unsafe {
            std::ptr::copy_nonoverlapping::<x86_thread_state64_t>(
                &mut thread_state,
                std::mem::transmute(&mut context),
                1,
            );
        }

        let results = unsafe { callback(&mut context) };
        thread.resume().unwrap();
        results
    }
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

/// An Unwinder walks one stack and collects frames.
///
/// An unwinder should be created, then passed the context to unwind and finally one can retrieve the
/// frames.
///
/// Creation should be done outside the suspend_and_resume_thread call!
pub struct LibunwindUnwinder {
    frames: Sample,
}

impl LibunwindUnwinder {
    /// Creates a new Unwinder.
    ///
    /// This sample will hold upto max_frames frames.
    /// The collection begins from the bottom-most function on the stack, so once the limit is
    /// reached, top frames are dropped.
    ///
    /// This is NOT safe to use within suspend_and_resume_thread.
    pub fn new(max_frames: usize) -> Self {
        Self {
            frames: Vec::with_capacity(max_frames),
        }
    }
}

impl Unwinder<&mut unw::unw_context_t> for LibunwindUnwinder {
    // TODO: Use failure for better errors + wrap unwind errors.
    /// The length of the vector is the actual collected frames (<= max_frames).
    ///
    /// This IS safe to use within suspend_and_resume_thread.
    ///
    /// TODO: Right now if stepping fails, this whole function fails, but we may want to return the
    /// frames we have. We also probably want another state to indicate we had more frames than
    /// capacity, so users can report some kind of stats.
    fn unwind(mut self, context: &mut unw::unw_context_t) -> Result<Sample, i32> {
        // This is a stack allocation, so it is OK.
        let mut cursor: unw::unw_cursor_t = unsafe { mem::uninitialized() };
        let init = unsafe { unw::unw_init_local(&mut cursor, context) };
        if init < 0 {
            return Err(init);
        }
        loop {
            if self.frames.len() == self.frames.capacity() {
                break;
            }
            let step = unsafe { unw::unw_step(&mut cursor) };
            if step == 0 {
                // No more frames.
                break;
            } else if step < 0 {
                return Err(step);
            }

            let mut ip = 0;
            let rr = unsafe { unw::unw_get_reg(&mut cursor, unw::UNW_REG_IP, &mut ip) };
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
#[cfg(test)]
mod tests {
    extern crate rustc_demangle;
    extern crate std;

    use super::*;

    use self::rustc_demangle::demangle;
    use std::{
        sync::{mpsc::channel, Arc},
        thread::spawn,
    };

    #[test]
    fn test_suspend_resume() {
        let sampler = Sampler::new();
        let (tx, rx) = channel();
        // Just to get the thread to wait until the test is done.
        let (tx2, rx2) = channel();
        let handle = spawn(move || {
            tx.send(current_thread().unwrap()).unwrap();
            rx2.recv().unwrap();
        });

        let to = rx.recv().unwrap();
        sampler.suspend_and_resume_thread(to, |context| {
            // TODO: This is where we would want to use libunwind in a real program.
            let mut cursor: unw::unw_cursor_t = unsafe { mem::uninitialized() };
            let init = unsafe { unw::unw_init_local(&mut cursor, context) };
            assert!(init >= 0);
            let mut ip = 0;
            let rr = unsafe { unw::unw_get_reg(&mut cursor, unw::UNW_REG_IP, &mut ip) };
            assert_eq!(rr, 0);
            // we can tell the thread to shutdown once it is resumed.
            tx2.send(()).unwrap();
        });
        handle.join().unwrap();
    }

    #[test]
    #[should_panic]
    fn test_suspend_resume_itself() {
        let sampler = Sampler::new();
        let to = current_thread().unwrap();
        sampler.suspend_and_resume_thread(to, |_| {});
    }
}
