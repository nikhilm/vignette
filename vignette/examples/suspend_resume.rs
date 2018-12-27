extern crate threadinfo;
extern crate vignette;

use std::{
    cell::RefCell,
    sync::{Arc, RwLock},
    thread::spawn,
};

use threadinfo::{current_thread, thread_iterator};
use vignette::Sampler;

fn main() {
    // Spawn a bunch of threads, then sample them.
    let running = Arc::new(RwLock::new(true));
    let mut handles = Vec::new();
    for _ in 0..10 {
        let running2 = running.clone();
        handles.push(spawn(move || {
            while *(running2.read().unwrap()) {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }))
    }
    println!("Spawned {} threads", handles.len());

    let sampler = Sampler::new();
    let counter = RefCell::new(0);

    let threads = thread_iterator().expect("threads");
    for (i, thread) in threads.enumerate() {
        if thread.is_current_thread() {
            continue;
        }

        sampler.suspend_and_resume_thread(thread, |context| {
            *counter.borrow_mut() += 1;
            println!("Thread {} SP = {:p}", i, context.uc_stack.ss_sp);
        });
    }

    assert_eq!(
        *counter.borrow(),
        handles.len(),
        "expected all threads to be sampled once"
    );

    {
        let mut val = running.write().expect("write lock");
        *val = false;
    }

    for handle in handles {
        handle.join().unwrap();
    }

    println!("Done");
}
