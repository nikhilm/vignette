use std::ops::Range;

// we need to retrieve module name, GUID (build ID) and relative addr of IP.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModuleInfo {
    pub name: String,
    pub build_id: String,
}

pub type ModuleAndAddr = (ModuleInfo, usize);

#[derive(Debug, Clone)]
struct ExtraModuleInfo {
    range: Range<usize>,
    info: ModuleInfo,
}

/// TODO
pub struct ModuleCache {
    // The ModuleCache keeps a mapping from address ranges of IPs to a ModuleInfo instance.
    // Since it is not easy to know the upper limit of an IP, we instead preserve a start (base)
    // address as the lower and upper bound at the beginning, and extend the upper bound every time
    // we find that dladdr tells us a module we already know about. This is admittedly not
    // efficient, but we will see. This is also Linux specific, Mac and Windows may be able to tell
    // us the upper limit.
    module_ranges: Vec<ExtraModuleInfo>,
}

// TODO: write more tests
impl ModuleCache {
    pub fn new() -> Self {
        ModuleCache {
            module_ranges: Vec::new(),
        }
    }

    pub fn get_or_insert(&mut self, addr: u64) -> Option<ModuleAndAddr> {
        None
    }
}
