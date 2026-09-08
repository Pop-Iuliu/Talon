fn main() {
    println!("cargo:rustc-link-search=native=..");
    println!("cargo:rustc-link-search=native=.");
    println!("cargo:rustc-link-lib=dylib=if_nest");

    println!("cargo:rustc-link-arg=-Wl,-export-dynamic");

    println!("cargo:rerun-if-changed=../libif_nest.so");
}
