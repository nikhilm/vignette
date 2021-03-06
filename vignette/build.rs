#[cfg(target_os = "macos")]
use std::{env, path::PathBuf};

#[cfg(target_os = "macos")]
fn main() {
    println!("cargo:rerun-if-changed=macos/unwind_wrapper.h");

    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    let bindings = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header("macos/unwind_wrapper.h")
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("unwind_bindings.rs"))
        .expect("Couldn't write bindings!");
}

#[cfg(not(target_os = "macos"))]
fn main() {}
