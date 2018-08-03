extern crate vignette;

use std::io::Read;
use std::{
    sync::{Arc, RwLock},
    thread::spawn,
};

use vignette::{get_current_thread, is_current_thread, thread_iterator, Frame, Sample, Sampler};

fn fun_one(running2: Arc<RwLock<bool>>) {
    while *(running2.read().unwrap()) {
        let mut _sum = 0;
        for i in 1..10000 {
            _sum += i;
        }
    }
    println!("fun thread {:?}", get_current_thread());
}

fn boring_one(running2: Arc<RwLock<bool>>) {
    while *(running2.read().unwrap()) {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    println!("boring thread {:?}", get_current_thread());
}

fn sample_once(sampler: &Sampler, thread: &vignette::ThreadId) {
    let sample = Sample::new(20);
    // This is an abomination we should get rid of.
    let mut frames: Option<Vec<Frame>> = None;
    frames.get_or_insert(sampler.suspend_and_resume_thread(thread, move |context| {
        // TODO: For perf we probably actually want to allow re-use of the sample storage,
        // instead of allocating new frames above every time.
        // i.e. once a sample has been captured and turned into some other representation, we
        // could re-use the vector.
        sample.collect(context).expect("sample succeeded")
    }));

    println!("Thread {:?}", thread);
    for frame in frames.unwrap().iter() {
        println!("  IP: 0x{:x}", frame.ip);
    }
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

    let sampler = Sampler::new();

    let threads = thread_iterator().expect("threads");
    for res in threads {
        let thread = res.expect("thread");
        if is_current_thread(&thread) {
            continue;
        }
        for _ in 0..2 {
            sample_once(&sampler, &thread);
        }
    }

    {
        let mut val = running.write().expect("write lock");
        *val = false;
    }

    for handle in handles {
        handle.join().unwrap();
    }

    println!("Done");
    let mut file = std::fs::File::open("/proc/self/maps").unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents);
    println!("{}", contents);
}
