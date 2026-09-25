use std::env;
use std::path::PathBuf;

fn main() {
    let tests_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../Tests")
        .canonicalize()
        .expect("Tests/ with libif_nest.so must be built before cargo");

    println!("cargo:rustc-link-search=native={}", tests_dir.display());
    println!("cargo:rustc-link-lib=dylib=if_nest");
    println!("cargo:rustc-link-arg=-Wl,-export-dynamic");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", tests_dir.display());
    println!(
        "cargo:rerun-if-changed={}",
        tests_dir.join("libif_nest.so").display()
    );
}
