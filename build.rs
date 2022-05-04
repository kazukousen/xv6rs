fn main() {
    // Tell the Cargo that if the given file changes, to return the build script.
    println!("cargo:rerun-if-changed=src/entry.S");

    // use the `cc` crate to assemble the assembly file and statically link it.
    cc::Build::new()
        .file("src/entry.S")
        .compile("asm");
}

