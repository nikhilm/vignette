#![feature(range_contains)]

#[macro_use]
extern crate serde_derive;
extern crate threadinfo;

#[cfg(target_os = "linux")]
mod lib_linux;
#[cfg(target_os = "linux")]
pub use lib_linux::*;

#[cfg(target_os = "macos")]
mod lib_mac;
#[cfg(target_os = "macos")]
pub use lib_mac::*;

pub mod output;
pub mod speedscope;

pub mod types;
mod module_cache;

use std::collections::HashMap;

use threadinfo::Thread as ThreadId;
use types::{Frame, Sample, Unwinder};

pub struct Profiler {
    sampler: Sampler,
    // TODO: If we want to support programs that load and unload shared libraries, we will want to
    // capture the state of all modules at the time of profile capture. Then we'd have to have a
    // module cache here, and propagate it to the profile.
}

impl Profiler {
    pub fn new() -> Profiler {
        Profiler {
            // TODO: Once we have a start/stop based interface, move this construction to there,
            // since this overrides the signal handler for the process and is only needed later. We
            // probably want some session kind of concept.
            sampler: Sampler::new(),
        }
    }

    pub fn session(&self) -> Session {
        Session {
            profiler: &self,
            threads: HashMap::new(),
        }
    }
}

pub struct Session<'a> {
    profiler: &'a Profiler,
    threads: HashMap<ThreadId, Vec<Vec<Frame>>>,
}

impl<'a> Session<'a> {
    /// Samples one thread once.
    /// Panics if the thread is the sampling thread.
    pub fn sample_thread(&mut self, thread: ThreadId) {
        let sample = self.sample_once(thread);
        self.threads.entry(thread).or_insert_with(|| Vec::new()).push(sample);
    }

    fn sample_once(&self, thread: ThreadId) -> Vec<Frame> {
        // TODO: Want to make the sample sizes configurable.
        let mut unwinder = LibunwindUnwinder::new(150);
        // TODO: Need to think if this interface is the best.
        self.profiler.sampler.suspend_and_resume_thread(thread, move |context| {
            // TODO: For perf we probably actually want to allow re-use of the sample storage,
            // instead of allocating new frames above every time.
            // i.e. once a sample has been captured and turned into some other representation, we
            // could re-use the vector.
            unwinder.unwind(context).expect("sample succeeded")
        })
    }

    pub fn finish(self) -> Profile {
        Profile {
            threads: self.threads,
        }
    }
}

/// In-memory profile. This is just an opaque container for now.
/// Use the Outputter to obtain a serializable form with build IDs resolved.
pub struct Profile {
    threads: HashMap<ThreadId, Vec<Vec<Frame>>>,
}

// TODO: Can we also have an iterator interface where each iteration causes a sampling? That way it
// would be lazy.

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc::channel, thread::spawn};

    #[cfg(target_os = "linux")]
    #[test]
    fn test_suspend_resume() {
        let sampler = Sampler::new();
        let (tx, rx) = channel();
        // Just to get the thread to wait until the test is done.
        let (tx2, rx2) = channel();
        let handle = spawn(move || {
            tx.send(get_current_thread()).unwrap();
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
    }

}
