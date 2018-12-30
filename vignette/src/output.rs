use std::collections::HashMap;
use std::hash::Hash;

use super::module_cache::{ModuleCache, ModuleInfo};
use super::threadinfo::Thread as ThreadId;
use super::Frame as InputFrame;
use super::Profile as InputProfile;

// Intermediate vignette format to serialize instruction pointers and module caches without
// symbols. This is then converted to a format other tools can support once symbols are available.

// Obviously not an efficient output format.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
pub struct Frame {
    pub module_index: u32,
    pub relative_ip: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Sample {
    /// Index into a Vec<Frame>.
    pub frames: Vec<usize>,
}

pub type Samples = Vec<Sample>;

#[derive(Debug, Serialize, Deserialize)]
pub struct Thread {
    pub thread_id: ThreadId,
    pub samples: Samples,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub struct Module {
    pub name: String,
    pub build_id: String,
}

impl From<ModuleInfo> for Module {
    fn from(mi: ModuleInfo) -> Self {
        Self {
            name: mi.name,
            build_id: mi.build_id,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Profile {
    pub modules: Vec<Module>,
    pub threads: Vec<Thread>,
    pub frames: Vec<Frame>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
pub struct ResolvedFrame {
    pub name: String,
    pub file: String,
    pub line: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResolvedProfile {
    pub modules: Vec<Module>,
    pub threads: Vec<Thread>,
    pub frames: Vec<ResolvedFrame>,
}

// TODO: VecHashMap shouldn't be in output.
/// Want a structure where a list of unique items is maintained.
/// Callers can query this with a key and retrieve the index of that key in the list. This index is
/// guaranteed to never change. Otherwise it is inserted and the index retrieved.
pub struct VecHashMap<V>
where
    V: Hash + Eq + Clone,
{
    vec: Vec<V>,
    map: HashMap<V, usize>,
}

impl<V> VecHashMap<V>
where
    V: Hash + Eq + Clone,
{
    pub fn new() -> VecHashMap<V> {
        VecHashMap {
            vec: Vec::new(),
            map: HashMap::new(),
        }
    }

    pub fn get_or_insert(&mut self, item: V) -> usize {
        if self.map.contains_key(&item) {
            self.map[&item]
        } else {
            self.vec.push(item.clone());
            let index = self.vec.len() - 1;
            self.map.insert(item, index);
            index
        }
    }

    /// Returns a copy of the current state of the vector.
    pub fn vec(&self) -> Vec<V> {
        self.vec.clone()
    }
}

// TODO: The current interface allows mixing multiple profile outputs into one outputter. we
// probably want to prevent that.
pub struct Outputter {
    // Used to go from IP-only frames to frames with a module index and relative offset.
    module_cache: ModuleCache,
    // Used to get a reduced serializable profile, where there is a common list of loaded modules
    // and sampled frames. Each frame in frames index refers to a module in module_index by index.
    // Each sample in the thread samples refers to a frame by the frame index.
    module_index: VecHashMap<ModuleInfo>,
    frames_index: VecHashMap<Frame>,
}

/// This is meant to be shared across multiple profilers/profiles in a process for now, under the
/// assumption that loaded modules in a program do not change.
impl Outputter {
    pub fn new() -> Outputter {
        Outputter {
            module_cache: ModuleCache::new(),
            module_index: VecHashMap::new(),
            frames_index: VecHashMap::new(),
        }
    }

    fn output_frame(&mut self, frame: InputFrame) -> Option<Frame> {
        match self
            .module_cache
            .get_or_insert(frame.ip)
        {
            Some((module, rva)) => {
                let module_pos = self.module_index.get_or_insert(module.clone());
                Some(Frame {
                    module_index: module_pos as u32,
                    relative_ip: rva as u64,
                })
            }
            None => {
                // TODO: error propagation/indication.
                eprintln!("ip: 0x{:x}", frame.ip);
                None
            }
        }
    }

    // TODO: Need some way to represent a frame that didn't match to any module.
    fn output_sample(&mut self, sample: Vec<InputFrame>) -> Sample {
        let mut output_frames = Vec::with_capacity(sample.len());
        for frame in sample {
            let output_frame = self.output_frame(frame);
            if let Some(output_frame) = output_frame {
                let frame_pos = self.frames_index.get_or_insert(output_frame);
                output_frames.push(frame_pos);
            }
        }
        Sample {
            frames: output_frames,
        }
    }

    pub fn output(&mut self, profile: InputProfile) -> Profile {
        let mut threads = Vec::new();
        for (thread_id, samples) in profile.threads {
            let mut output_samples = Vec::with_capacity(samples.len());
            for sample in samples {
                output_samples.push(self.output_sample(sample));
            }

            threads.push(Thread {
                thread_id: thread_id,
                samples: output_samples,
            });
        }

        Profile {
            threads: threads,
            modules: self
                .module_index
                .vec()
                .into_iter()
                .map(Module::from)
                .collect(),
            frames: self.frames_index.vec(),
        }
    }
}
