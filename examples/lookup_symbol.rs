/// Given an address and the location of breakpad generated symbols, tries to find the relevant
/// symbol.
extern crate breakpad_symbols;

use breakpad_symbols::SymbolFile;

const USAGE: &str = "<addr hex with 0x prefix> <path to sym>";

fn main() {
    let mut args = std::env::args();

    let prog = args.next().expect("program itself");
    let addr_s = match args.next() {
        Some(x) => x,
        None => panic!("{} {}", USAGE, prog),
    };
    let sympath = match args.next() {
        Some(x) => x,
        None => panic!("{} {}", USAGE, prog),
    };

    SymbolFile::from_file(sympath).expect("valid sym file");
}
