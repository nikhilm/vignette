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

use vignette::module_cache::ModuleCache;
use vignette::{
    get_current_thread, is_current_thread, thread_iterator, Frame, Sample, Sampler, ThreadId,
};

mod output {
    use vignette::ThreadId;

    // Obviously not an efficient output format.
    #[derive(Debug, Serialize, Deserialize)]
    pub struct Frame {
        pub module_index: u32,
        pub relative_ip: u64,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Sample {
        pub frames: Vec<Frame>,
    }

    pub type Samples = Vec<Sample>;

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Thread {
        pub thread_id: ThreadId,
        pub samples: Samples,
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
    pub struct Module {
        pub name: String,
        pub build_id: String,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Profile {
        pub modules: Vec<Module>,
        pub threads: Vec<Thread>,
    }
}

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

    let mut thread_map: HashMap<ThreadId, output::Samples> = HashMap::new();
    let mut module_list = Vec::new();
    let threads = thread_iterator().expect("threads");
    for res in threads {
        let thread = res.expect("thread");
        if is_current_thread(&thread) {
            continue;
        }
        let frames = sample_once(&sampler, &thread);
        let mut sample = output::Sample { frames: vec![] };
        for frame in frames {
            match module_cache.get_or_insert(frame.ip as *const libc::c_void) {
                Some((module, rva)) => {
                    let out_mod = output::Module {
                        name: module.name,
                        build_id: module.build_id,
                    };
                    let mut index = 0;
                    let mut search = module_list.iter().position(|p| *p == out_mod);
                    match search {
                        Some(i) => index = i,
                        None => {
                            index = module_list.len();
                            module_list.push(out_mod);
                        }
                    }
                    sample.frames.push(output::Frame {
                        module_index: index as u32,
                        relative_ip: rva as u64,
                    });
                }
                None => {
                    eprintln!("ip: 0x{:x}", frame.ip);
                }
            }
        }
        let samples: output::Samples = vec![sample];
        thread_map.insert(thread, samples);
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

    let profile = output::Profile {
        modules: module_list,
        threads: threads,
    };

    let filename = format!("{}.vignette", process::id());
    let mut file = File::create(&filename).unwrap();
    serde_json::to_writer_pretty(file, &profile).unwrap();
    println!("Wrote to {}", filename);
    // file.write_all().unwrap();
    //
    // TODO: Move this module lookup and serialization stuff to a module and add tests.
}
