extern crate mach;

use std::{
    io::{Error, ErrorKind, Result},
    iter::Iterator,
};

use mach::{
    kern_return::KERN_SUCCESS,
    mach_init::mach_thread_self,
    mach_types::{thread_act_array_t, thread_act_t},
    message::mach_msg_type_number_t,
    task::task_threads,
    traps::mach_task_self,
};
use mach::thread_act::thread_suspend;
use mach::thread_act::thread_resume;

// TODO: Better way than exposing private member?
// Needed so vignette can retrieve the context
#[derive(Eq, PartialEq, Debug, Hash, Copy, Clone, Serialize, Deserialize)]
pub struct Thread(pub thread_act_t);

impl Thread {
    pub fn is_current_thread(&self) -> bool {
        self == &current_thread().expect("current thread should never fail")
    }

    pub fn suspend(&self) -> Result<()> {
        let r = unsafe { thread_suspend(self.0)};
        if r == KERN_SUCCESS {
            Ok(())
        } else {
            Err(Error::new(ErrorKind::Other, format!("Could not suspend ({})", r)))
        }
    }

    pub fn resume(&self) -> Result<()> {
        let r = unsafe { thread_resume(self.0)};
        if r == KERN_SUCCESS {
            Ok(())
        } else {
            Err(Error::new(ErrorKind::Other, format!("Could not resume ({})", r)))
        }
    }
}

/// Returns an object for the current thread.
pub fn current_thread() -> Result<Thread> {
    let tid = unsafe { mach_thread_self() };
    Ok(Thread(tid))
}

/// Returns an iterator over the current process' threads.
///
/// This function does not guarantee that the threads it returns are the complete and full set of
/// threads in the process. Threads may be created that are not in the iterator, and threads
/// returned from this iterator may have terminated.
pub fn thread_iterator() -> Result<impl Iterator<Item = Thread>> {
    let task = unsafe { mach_task_self() };
    let mut threads: thread_act_array_t = std::ptr::null_mut();
    let mut thread_count: mach_msg_type_number_t = 0;
    let ret = unsafe { task_threads(task, &mut threads, &mut thread_count) };
    if ret == KERN_SUCCESS {
        let thread_count_sized = thread_count as usize;
        assert!(thread_count_sized > 0);
        let mut result = Vec::with_capacity(thread_count_sized);
        for i in 0..thread_count_sized {
            result.push(Thread(unsafe { *threads.offset(i as isize) }))
        }
        Ok(result.into_iter())
    } else {
        Err(Error::new(
            ErrorKind::Other,
            format!("Error retrieving threads ({})", ret),
        ))
    }
}
