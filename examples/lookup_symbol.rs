/// Given an address and the location of breakpad generated symbols, tries to find the relevant
/// symbol.
extern crate breakpad_symbols;
extern crate rustc_demangle;
use self::rustc_demangle::demangle;

use breakpad_symbols::SymbolFile;
use std::path::Path;

const USAGE: &str = "<path to sym> [<addr hex without 0x prefix>]...";

fn main() {
    let mut args = std::env::args();

    let prog = args.next().expect("program itself");
    let sym_path = match args.next() {
        Some(x) => x,
        None => panic!("{} {}", prog, USAGE),
    };

    let sym_file = SymbolFile::from_file(Path::new(&sym_path)).expect("valid sym file");
    for addr in args {
        let ip = u64::from_str_radix(&addr, 16).expect("valid hex address");
        let sym = sym_file.find_nearest_public(ip).expect("found some symbol");
        println!("Symbol: {}", demangle(&sym.name));
    }
}
