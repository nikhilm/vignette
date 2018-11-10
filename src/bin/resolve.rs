extern crate serde;
extern crate serde_json;
extern crate symbolic_common;
extern crate symbolic_debuginfo;
extern crate symbolic_symcache;

extern crate vignette;

use std::collections::HashMap;
use std::io::Read;
use std::io::Write;
use symbolic_common::byteview::ByteView;
use symbolic_common::types::ObjectKind;
use symbolic_debuginfo::{BreakpadData, BreakpadRecord, FatObject};
use symbolic_symcache::SymCache;
use vignette::output;

struct SymCacheCache<'a> {
    // Option because we may be unable to load a symbol file/cache for a given module. We do not
    // retry in that case.
    module_to_cache: HashMap<output::Module, Option<SymCache<'a>>>,
    symbol_root: String,
}

impl<'a> SymCacheCache<'a> {
    pub fn new<'b>(sym_root: String) -> SymCacheCache<'b> {
        SymCacheCache {
            module_to_cache: HashMap::new(),
            symbol_root: sym_root,
        }
    }

    pub fn get_or_create_cache(&mut self, module: output::Module) -> &Option<SymCache> {
        if !self.module_to_cache.contains_key(&module) {
            let mut sym_path = std::path::PathBuf::from(&self.symbol_root);
            sym_path.push(&module.name);
            sym_path.push(&module.build_id);
            sym_path.push(format!("{}.sym", module.name));
            let mut file = match std::fs::OpenOptions::new().read(true).open(sym_path) {
                Ok(f) => f,
                Err(e) => {
                    writeln!(std::io::stderr(), "{:?}", e);
                    return &None;
                }
            };
            let mut contents = Vec::new();
            file.read_to_end(&mut contents).expect("read file");

            let fat_object =
                FatObject::parse(ByteView::from_vec(contents)).expect("valid fatobject");
            assert_eq!(fat_object.kind(), ObjectKind::Breakpad);
            assert_eq!(fat_object.object_count(), 1);
            let object = fat_object.get_object(0).unwrap().unwrap();
            let cache = SymCache::from_object(&object).expect("valid cache");
            self.module_to_cache.insert(module.clone(), Some(cache));
        }
        self.module_to_cache.get(&module).unwrap()
    }

    pub fn lookup_symbol(
        &mut self,
        module: &output::Module,
        relative_ip: u64,
    ) -> Option<(String, String, u32)> {
        let symcache = self.get_or_create_cache((*module).clone());
        if symcache.is_none() {
            return None;
        }

        let symcache = symcache.as_ref().unwrap();
        let lookup_result = symcache.lookup(relative_ip);
        if lookup_result.is_err() {
            return None;
        }
        lookup_result
            .unwrap()
            .first()
            .map(|x| (x.function_name(), x.filename().to_owned(), x.line()))
    }
}

fn resolve_frame(
    unresolved_frame: &output::Frame,
    modules: &Vec<output::Module>,
    symcache: &mut SymCacheCache,
) -> output::ResolvedFrame {
    let module = &modules[unresolved_frame.module_index as usize];

    let (function, file, line) = symcache
        .lookup_symbol(module, unresolved_frame.relative_ip)
        .unwrap_or_else(|| ("unknown".to_owned(), "unknown".to_owned(), 0));

    output::ResolvedFrame {
        name: function,
        file: file,
        line: line,
    }
}

fn main() {
    let mut args = std::env::args();
    args.next().expect("the program itself");
    let unresolved_profile_path = args.next().expect("profile");
    let symbol_root = args.next().expect("symbols location");

    let unresolved_profile: output::Profile = serde_json::from_reader(
        std::fs::OpenOptions::new()
            .read(true)
            .open(unresolved_profile_path)
            .expect("valid file"),
    )
    .expect("valid profile");

    let mut symcache = SymCacheCache::new(symbol_root);

    let resolved_frames: Vec<output::ResolvedFrame> = unresolved_profile
        .frames
        .as_ref()
        .expect("frames")
        .iter()
        .map(|f| resolve_frame(f, &unresolved_profile.modules, &mut symcache))
        .collect();

    // Translate frames to resolved frames, looking up modules as required.
    let resolved_profile = output::Profile {
        modules: unresolved_profile.modules,
        threads: unresolved_profile.threads,
        frames: None,
        resolved_frames: Some(resolved_frames),
    };
    let stdout = std::io::stdout();
    serde_json::to_writer_pretty(stdout.lock(), &resolved_profile).expect("wrote resolved profile");
}
