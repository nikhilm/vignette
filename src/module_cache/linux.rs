extern crate goblin;
extern crate hex;
extern crate libc;
extern crate proc_maps;

use self::goblin::elf::header::header64::{Header, SIZEOF_EHDR};
use self::goblin::elf::note::NT_GNU_BUILD_ID;
use self::goblin::elf::program_header::program_header64::ProgramHeader;
use self::goblin::elf::section_header::section_header64::SectionHeader;
use self::goblin::elf::section_header::SHT_NOTE;
use self::goblin::elf::Elf;
use self::proc_maps::get_process_maps;
use std::ffi::CStr;
use std::fs::File;
use std::io::Read;
use std::io::Result;
use std::mem;
use std::ops::Range;
use std::path::Path;
use std::process;
use std::slice;

// we need to retrieve module name, GUID (build ID) and relative addr of IP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleInfo {
    pub name: String,
    pub build_id: String,
}

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

impl ModuleCache {
    pub fn new() -> Self {
        ModuleCache {
            module_ranges: Vec::new(),
        }
    }

    fn find_existing(&self, addr: *const libc::c_void) -> Option<ModuleInfo> {
        let existing = self.module_ranges.iter().find(|module| {
            eprintln!("Searching existing {:?}", module);
            module.range.contains(&(addr as usize))
        });
        existing.map(|x| x.info.clone())
    }

    pub fn get_or_insert(&mut self, addr: *const libc::c_void) -> Option<ModuleInfo> {
        if let Some(module) = self.find_existing(addr) {
            eprintln!("Found existing {:?} for {}", module, addr as usize);
            return Some(module);
        }

        eprintln!("Not found");

        let mut mod_info: libc::Dl_info = unsafe { mem::uninitialized() };
        let r = unsafe { libc::dladdr(addr as *const libc::c_void, &mut mod_info) };
        if r == 0 {
            // No matching shared object.
            return None;
        }

        let cpath = unsafe { CStr::from_ptr(mod_info.dli_fname) };
        let path = Path::new(cpath.to_str().expect("valid path"));
        let mut file = File::open(&path).expect("valid file");
        let mut contents = Vec::new();
        file.read_to_end(&mut contents);
        let elf = Elf::parse(&contents).expect("valid elf");
        // aaaaa! go back to possibly parsing file section by section and doing the string table
        // lookup ourselves.
        let mut notes = elf.iter_note_sections(&contents, None).unwrap();
        let mut build_id_opt = None;
        for note_r in notes {
            let note = note_r.unwrap();
            if note.name != "GNU" {
                continue;
            }

            if note.n_type != NT_GNU_BUILD_ID {
                continue;
            }

            build_id_opt = Some(hex::encode_upper(note.desc));
            break;
        }

        if build_id_opt.is_none() {
            // Could not retrieve build ID.
            return None;
        }

        let build_id = build_id_opt.unwrap();
        let base = mod_info.dli_fbase as usize;
        eprintln!("Path {:?}", path);

        // Try to find a module info matching these attributes and adjust it's range if required.
        // Otherwise insert one.
        // Return it in both cases.
        let name = path.file_name().expect("file name").to_str().expect("utf8");
        {
            let mut existing = self.module_ranges.iter_mut().find(|module| {
                eprintln!("searching {:?}", module);
                module.info.name == name
                    && module.info.build_id == build_id
                    && module.range.start == base
            });

            if let Some(module) = existing {
                eprintln!("found existing module, gonna fix range");
                assert!(addr as usize >= module.range.start);
                module.range = (module.range.start..(addr as usize).max(module.range.end));
                eprintln!("fixed module {:?}", module);
                return Some(module.info.clone());
            }
        }

        let new = ExtraModuleInfo {
            range: (base..(addr as usize) + 1),
            info: ModuleInfo {
                name: name.to_string(),
                build_id: build_id,
            },
        };

        eprintln!("Gonna insert new {:?}", new);
        self.module_ranges.push(new.clone());
        Some(new.info)
    }
}

#[cfg(test)]
mod tests {
    extern crate libc;
    use super::ModuleCache;
    use std::env;
    use std::ffi::CString;
    use std::process;

    #[test]
    fn test_cache() {
        let mut cache = ModuleCache::new();
        let entry = cache.get_or_insert((&ModuleCache::new as *const _) as *const libc::c_void);
        assert!(entry.is_some());

        let pth = CString::new("/lib/x86_64-linux-gnu/libpthread.so.0").unwrap();
        let handle = unsafe { libc::dlopen(pth.as_ptr(), libc::RTLD_LAZY) };
        assert!(!handle.is_null());

        let mutex_init = CString::new("pthread_mutex_init").unwrap();
        let mutex_init_addr = unsafe { libc::dlsym(handle, mutex_init.as_ptr()) };
        eprintln!("pthread_mutex_init {:?}", mutex_init_addr);
        let pthread_entry = cache.get_or_insert(mutex_init_addr).unwrap();
        assert!(pthread_entry.name == "libpthread.so.0");

        let mutex_destroy = CString::new("pthread_mutex_destroy").unwrap();
        let mutex_destroy_addr = unsafe { libc::dlsym(handle, mutex_destroy.as_ptr()) };
        eprintln!("pthread_mutex_destroy {:?}", mutex_destroy_addr);
        let pthread_entry2 = cache.get_or_insert(mutex_destroy_addr).unwrap();
        assert!(pthread_entry == pthread_entry2);
    }

    #[test]
    fn test_rva() {
        let cache = ModuleCache::new();
    }
}
