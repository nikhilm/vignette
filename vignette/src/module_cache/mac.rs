extern crate goblin;
extern crate hex;
extern crate libc;
extern crate memmap;

use self::{
    goblin::mach::{constants::cputype::CPU_TYPE_X86_64, load_command::CommandVariant, Mach},
    memmap::MmapOptions,
};
use std::{ffi::CStr, fs::File, mem, ops::Range, path::Path};

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

    fn find_existing(&self, addr: usize) -> Option<ExtraModuleInfo> {
        let existing = self
            .module_ranges
            .iter()
            .find(|module| module.range.contains(&(addr as usize)));
        existing.map(|x| x.clone())
    }

    fn relative_addr(info: &ExtraModuleInfo, addr: usize) -> usize {
        assert!(info.range.contains(&(addr as usize)));
        return (addr as usize) - info.range.start;
    }

    pub fn get_or_insert(&mut self, addr: usize) -> Option<ModuleAndAddr> {
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
        // TODO: Reformat note for mach-o or actually implement direct binding read.
        // Theoretically, SHT_NOTE gets converted to PT_NOTE, which should always be loaded, so we
        // can manually walk ELF headers and Phdrs to extract a build ID without having to re-mmap
        // each file. Goblin can't do it since it is expecting a complete ELF file, but perhaps we
        // can use segments of it. Something to optimize in the future.
        let path = Path::new(cpath.to_str().expect("valid path"));
        let file = File::open(&path).expect("valid file");
        let mapped = unsafe { MmapOptions::new().map(&file).expect("mmap") };
        let parsed = Mach::parse(&mapped).expect("mach-o");
        let macho = (match parsed {
            Mach::Fat(march) => {
                let mut idx = None;
                for (i, arch) in march.iter_arches().enumerate() {
                    if arch.is_ok() {
                        if arch.unwrap().cputype() == CPU_TYPE_X86_64 {
                            idx = Some(i);
                            break;
                        }
                    }
                }
                idx.map(|i| march.get(i))
            }
            Mach::Binary(macho) => {
                if macho.header.cputype() == CPU_TYPE_X86_64 {
                    Some(Ok(macho))
                } else {
                    None
                }
            }
            _ => None,
        })
        .expect("non-none")
        .expect("valid mach-o");

        let mut build_id_opt = None;
        for cmd in macho.load_commands {
            match cmd.command {
                CommandVariant::Uuid(uuid_command) => {
                    build_id_opt = Some(hex::encode_upper(uuid_command.uuid));
                    eprintln!("{:?}", build_id_opt);
                    break;
                }
                _ => {}
            }
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
            let existing = self.module_ranges.iter_mut().find(|module| {
                module.info.name == name
                    && module.info.build_id == build_id
                    && module.range.start == base
            });

            if let Some(module) = existing {
                assert!(addr as usize >= module.range.start);
                module.range = module.range.start..((addr as usize) + 1).max(module.range.end);
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
