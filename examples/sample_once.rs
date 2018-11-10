extern crate libc;
extern crate vignette;

#[macro_use]
extern crate serde_derive;

extern crate serde;
extern crate serde_json;

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::process;
use std::{
    sync::{Arc, RwLock},
    thread::spawn,
};

use vignette::module_cache::{ModuleCache, ModuleInfo};
use vignette::output;
use vignette::{
    get_current_thread, is_current_thread, thread_iterator, Frame, Sample, Sampler, ThreadId,
};

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

fn sample_once(sampler: &Sampler, thread: &vignette::ThreadId) -> Vec<Frame> {
    let sample = Sample::new(20);
    let mut frames = sampler.suspend_and_resume_thread(thread, move |context| {
        // TODO: For perf we probably actually want to allow re-use of the sample storage,
        // instead of allocating new frames above every time.
        // i.e. once a sample has been captured and turned into some other representation, we
        // could re-use the vector.
        sample.collect(context).expect("sample succeeded")
    });

    frames
}

fn output_sample(
    frames: Vec<Frame>,
    module_cache: &mut ModuleCache,
    module_index: &mut output::VecHashMap<ModuleInfo>,
    frames_index: &mut output::VecHashMap<output::Frame>,
) -> output::Sample {
    let mut sample = output::Sample { frames: vec![] };
    for frame in frames {
        match module_cache.get_or_insert(frame.ip as *const libc::c_void) {
            Some((module, rva)) => {
                let module_pos = module_index.get_or_insert(module.clone());
                let output_frame = output::Frame {
                    module_index: module_pos as u32,
                    relative_ip: rva as u64,
                };
                let frame_pos = frames_index.get_or_insert(output_frame);
                sample.frames.push(frame_pos);
            }
            None => {
                eprintln!("ip: 0x{:x}", frame.ip);
            }
        }
    }
    sample
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
    let mut module_cache = ModuleCache::new();
    let mut module_index: output::VecHashMap<ModuleInfo> = output::VecHashMap::new();
    let mut frames_index: output::VecHashMap<output::Frame> = output::VecHashMap::new();

    let mut thread_map: HashMap<ThreadId, output::Samples> = HashMap::new();
    for i in 0..20 {
        let threads = thread_iterator().expect("threads");
        for res in threads {
            let thread = res.expect("thread");
            if is_current_thread(&thread) {
                continue;
            }
            let frames = sample_once(&sampler, &thread);
            let sample = output_sample(
                frames,
                &mut module_cache,
                &mut module_index,
                &mut frames_index,
            );
            let samples = thread_map.entry(thread).or_insert_with(|| Vec::new());
            samples.push(sample);
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

    let mut threads = vec![];
    for (thread_id, samples) in thread_map.into_iter() {
        threads.push(output::Thread { thread_id, samples });
    }

    let module_list: Vec<output::Module> = module_index
        .to_vec()
        .into_iter()
        .map(|module_info| output::Module {
            name: module_info.name,
            build_id: module_info.build_id,
        })
        .collect();
    let profile = output::Profile {
        modules: module_list,
        threads: threads,
        frames: Some(frames_index.to_vec()),
        resolved_frames: None,
    };

    let filename = format!("{}.vignette", process::id());
    let mut file = File::create(&filename).unwrap();
    serde_json::to_writer_pretty(file, &profile).unwrap();
    println!("Wrote to {}", filename);
}
