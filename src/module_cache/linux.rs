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
use std::process;
use std::slice;

// we need to retrieve module name, GUID (build ID) and relative addr of IP.
pub struct ModuleInfo {
    name: String,
    build_id: String,
}

pub struct ModuleCache {}

impl ModuleCache {
    pub fn new() -> Self {
        ModuleCache {}
    }

    fn info_for_addr<T>(&self, addr: *const T) -> Option<ModuleInfo> {
        let mut mod_info: libc::Dl_info = unsafe { mem::uninitialized() };
        let r = unsafe { libc::dladdr(addr as *const libc::c_void, &mut mod_info) };
        if r == 0 {
            // No matching shared object.
            return None;
        }

        let cpath = unsafe { CStr::from_ptr(mod_info.dli_fname) };
        let path = cpath.to_str().expect("valid path");
        eprintln!("{}", path);
        let mut file = File::open(path).expect("valid file");
        let mut contents = Vec::new();
        file.read_to_end(&mut contents);
        let elf = Elf::parse(&contents).expect("valid elf");
        // aaaaa! go back to possibly parsing file section by section and doing the string table
        // lookup ourselves.
        let mut notes = elf.iter_note_sections(&contents, None).unwrap();
        for note_r in notes {
            let note = note_r.unwrap();
            if note.name != "GNU" {
                continue;
            }

            if note.n_type != NT_GNU_BUILD_ID {
                continue;
            }

            eprintln!("{}", hex::encode_upper(note.desc));
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::ModuleCache;
    use std::env;

    #[test]
    fn test_cache() {
        let cache = ModuleCache::new();
        let entry = cache.info_for_addr(&ModuleCache::new);
        assert!(entry.is_some());
    }

    #[test]
    fn test_rva() {
        let cache = ModuleCache::new();
    }
}
