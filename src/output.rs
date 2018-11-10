use std::collections::HashMap;
use std::hash::Hash;

use ThreadId;

// Intermediate vignette format to serialize instruction pointers and module caches without
// symbols. This is then converted to a format other tools can support once symbols are available.

// Obviously not an efficient output format.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
pub struct Frame {
    pub module_index: u32,
    pub relative_ip: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
pub struct ResolvedFrame {
    pub name: String,
    pub file: String,
    pub line: u32,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct Profile {
    pub modules: Vec<Module>,
    pub threads: Vec<Thread>,
    pub frames: Option<Vec<Frame>>,
    pub resolved_frames: Option<Vec<ResolvedFrame>>,
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

    pub fn to_vec(self) -> Vec<V> {
        self.vec
    }
}
