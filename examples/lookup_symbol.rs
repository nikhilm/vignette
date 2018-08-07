/// Given an address and the location of breakpad generated symbols, tries to find the relevant
/// symbol.
extern crate breakpad_symbols;

use breakpad_symbols::SymbolFile;
use std::path::Path;

const USAGE: &str = "<path to sym> <addr hex without 0x prefix>";

fn main() {
    let mut args = std::env::args();

    let prog = args.next().expect("program itself");
    let sym_path = match args.next() {
        Some(x) => x,
        None => panic!("{} {}", prog, USAGE),
    };
    let addr = match args.next() {
        Some(x) => u64::from_str_radix(&x, 16).expect("valid hex address"),
        None => panic!("{} {}", prog, USAGE),
    };

    let sym_file = SymbolFile::from_file(Path::new(&sym_path)).expect("valid sym file");
    let sym = sym_file
        .find_nearest_public(addr)
        .expect("found some symbol");
    println!("Symbol: {:?}", sym);
}
