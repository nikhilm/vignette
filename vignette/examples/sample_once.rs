extern crate libc;
extern crate threadinfo;
extern crate vignette;

extern crate serde;
extern crate serde_json;

use std::fs::File;
use std::process;
use std::{
    sync::{Arc, RwLock},
    thread::spawn,
};

use vignette::output::Outputter;
// TODO: Everything except profiler really should not be public and we instead want to register
// threads with the profiler? The thread iteration stuff is useful in general though and could be a
// separate crate/module.
use threadinfo::{current_thread, thread_iterator};
use vignette::Profiler;

fn fun_one(running2: Arc<RwLock<bool>>) {
    while *(running2.read().unwrap()) {
        let mut _sum = 0;
        for i in 1..10000 {
            _sum += i;
        }
    }
    println!("fun thread {:?}", current_thread().unwrap());
}

fn boring_one(running2: Arc<RwLock<bool>>) {
    while *(running2.read().unwrap()) {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    println!("boring thread {:?}", current_thread().unwrap());
}

fn main() {
    // Spawn a bunch of threads, then sample them.
    let running = Arc::new(RwLock::new(true));
    let mut handles = Vec::new();
    for i in 0..2 {
        let running2 = running.clone();
        handles.push(if i % 2 == 0 {
            spawn(move || {
                boring_one(running2);
            })
        } else {
            spawn(move || {
                fun_one(running2);
            })
        })
    }

    println!("Spawned {} threads", handles.len());

    // Let both threads start.
    std::thread::sleep(std::time::Duration::from_millis(100));

    let mut profiler = Profiler::new();

    for _ in 0..20 {
        let threads = thread_iterator().expect("threads");
        for thread in threads {
            // TODO: Kinda weird to leak this current thread detail out here.
            if thread.is_current_thread() {
                continue;
            }
            profiler.sample_thread(thread);
        }
    }

    {
        let mut val = running.write().expect("write lock");
        *val = false;
    }

    for handle in handles {
        handle.join().unwrap();
    }

    println!("Done sampling");

    let mut outputter = Outputter::new();
    let output_profile = outputter.output(profiler.finish());
    let filename = format!("{}.vignette", process::id());
    let file = File::create(&filename).unwrap();
    serde_json::to_writer_pretty(file, &output_profile).unwrap();
    println!("Wrote to {}", filename);
}
