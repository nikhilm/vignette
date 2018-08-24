use ThreadId;

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
