// Generates a symbol file in a given location, assuming dump_syms is in the path.

use std::io::BufRead;
use std::io::BufReader;
use std::io::Cursor;
use std::io::Write;
use std::path::{Path, PathBuf};

fn main() {
    let mut args = std::env::args();
    args.next().expect("the program itself");
    let binary = args.next().expect("binary");
    let symbol_root = args.next().expect("symbols location");

    let mut dump_syms = std::process::Command::new("dump_syms");
    dump_syms.arg(binary);

    let output = dump_syms.output().expect("command executed");
    if !output.status.success() {
        panic!("dump_syms failed");
    }

    let mut reader = BufReader::new(Cursor::new(&output.stdout));

    let header: Vec<String> = reader
        .lines()
        .take(2)
        .map(|res| res.expect("valid line"))
        .collect();

    // MODULE Linux x86_64 1428AABFDBD2A52E08B6D967319FB6FE0 sample_once
    assert!(header[0].starts_with("MODULE"));
    let binary_name = header[0].split(" ").nth(4).expect("binary name");
    // INFO CODE_ID BFAA2814D2DB2EA508B6D967319FB6FEF5B14C2C
    assert!(header[1].starts_with("INFO CODE_ID"));
    let build_id = header[1].split(" ").nth(2).expect("build id");

    let mut sym_path = PathBuf::from(symbol_root);
    sym_path.push(binary_name);
    sym_path.push(build_id);

    std::fs::create_dir_all(&sym_path).expect("created dirs");
    sym_path.push(format!("{}.sym", binary_name));

    let mut output_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(sym_path)
        .expect("opened file");
    output_file.write(&output.stdout);
}
