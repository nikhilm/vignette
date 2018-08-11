extern crate goblin;
extern crate hex;
extern crate libc;

use self::goblin::elf::header::header64::{Header, SIZEOF_EHDR};
use self::goblin::elf::note::NT_GNU_BUILD_ID;
use self::goblin::elf::program_header::program_header64::ProgramHeader;
use self::goblin::elf::section_header::section_header64::SectionHeader;
use self::goblin::elf::section_header::SHT_NOTE;
use self::goblin::elf::Elf;
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

    fn find_existing(&self, addr: *const libc::c_void) -> Option<ExtraModuleInfo> {
        let existing = self
            .module_ranges
            .iter()
            .find(|module| module.range.contains(&(addr as usize)));
        existing.map(|x| x.clone())
    }

    fn relative_addr(info: &ExtraModuleInfo, addr: *const libc::c_void) -> usize {
        assert!(info.range.contains(&(addr as usize)));
        return (addr as usize) - info.range.start;
    }

    pub fn get_or_insert(&mut self, addr: *const libc::c_void) -> Option<ModuleAndAddr> {
        if let Some(module) = self.find_existing(addr) {
            let rva = Self::relative_addr(&module, addr);
            return Some((module.info.clone(), rva));
        }

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

        // Try to find a module info matching these attributes and adjust it's range if required.
        // Otherwise insert one.
        // Return it in both cases.
        let name = path.file_name().expect("file name").to_str().expect("utf8");
        {
            let mut existing = self.module_ranges.iter_mut().find(|module| {
                module.info.name == name
                    && module.info.build_id == build_id
                    && module.range.start == base
            });

            if let Some(module) = existing {
                assert!(addr as usize >= module.range.start);
                module.range = (module.range.start..((addr as usize) + 1).max(module.range.end));
                let rva = Self::relative_addr(&module, addr);
                return Some((module.info.clone(), rva));
            }
        }

        let new = ExtraModuleInfo {
            range: (base..(addr as usize) + 1),
            info: ModuleInfo {
                name: name.to_string(),
                build_id: build_id,
            },
        };

        self.module_ranges.push(new.clone());
        let rva = Self::relative_addr(&new, addr);
        Some((new.info, rva))
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
        let (entry, rva) = cache
            .get_or_insert((&ModuleCache::new as *const _) as *const libc::c_void)
            .unwrap();
        assert_eq!(
            entry.name,
            env::current_exe()
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
        );
        eprintln!("RVA 0x{:x}", rva);

        let pth = CString::new("/lib/x86_64-linux-gnu/libpthread.so.0").unwrap();
        let handle = unsafe { libc::dlopen(pth.as_ptr(), libc::RTLD_LAZY) };
        assert!(!handle.is_null());

        let mutex_init = CString::new("pthread_mutex_init").unwrap();
        let mutex_init_addr = unsafe { libc::dlsym(handle, mutex_init.as_ptr()) };
        eprintln!("pthread_mutex_init {:?}", mutex_init_addr);
        let (pthread_entry, init_rva) = cache.get_or_insert(mutex_init_addr).unwrap();
        assert_eq!(pthread_entry.name, "libpthread.so.0");

        let mutex_destroy = CString::new("pthread_mutex_destroy").unwrap();
        let mutex_destroy_addr = unsafe { libc::dlsym(handle, mutex_destroy.as_ptr()) };
        eprintln!("pthread_mutex_destroy {:?}", mutex_destroy_addr);
        let (pthread_entry2, destroy_rva) = cache.get_or_insert(mutex_destroy_addr).unwrap();
        assert_eq!(pthread_entry, pthread_entry2);
        assert!(init_rva < destroy_rva);
        eprintln!("init RVA 0x{:x}", init_rva);
        eprintln!("destroy RVA 0x{:x}", destroy_rva);
    }

    #[test]
    fn test_rva() {
        let cache = ModuleCache::new();
    }
}
