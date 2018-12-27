extern crate libc;

use libc::{pid_t, syscall, SYS_gettid};
use std::io::Result;
use std::iter::Iterator;

#[derive(Eq, PartialEq, Debug, Hash, Copy, Clone, Serialize, Deserialize)]
pub struct Thread(pid_t);

impl Thread {
    pub fn is_current_thread(&self) -> bool {
        self == &current_thread().expect("current thread should never fail")
    }

    pub fn send_signal(&self, signal: i32) {
        unsafe {
            libc::syscall(libc::SYS_tgkill, std::process::id(), self.0, signal);
        }
    }
}

/// Returns an object for the current thread.
pub fn current_thread() -> Result<Thread> {
    let tid = unsafe { syscall(SYS_gettid) as pid_t };
    Ok(Thread(tid))
}

/// Returns an iterator over the current process' threads.
///
/// This function does not guarantee that the threads it returns are the complete and full set of
/// threads in the process. Threads may be created that are not in the iterator, and threads
/// returned from this iterator may have terminated.
pub fn thread_iterator() -> Result<impl std::iter::Iterator<Item = Thread>> {
    std::fs::read_dir("/proc/self/task").and_then(|entries| {
        Ok(entries.map(|entry| {
            let file = entry.unwrap().file_name();
            // Probably faster to send the get_thread thing a direct string from `path()`.
            let tid = file.to_str().unwrap().parse::<i32>().unwrap();
            Thread(tid)
        }))
    })
}
