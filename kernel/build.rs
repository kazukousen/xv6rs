fn main() {
    // Tell the Cargo that if the given file changes, to return the build script.
    println!("cargo:rerun-if-changed=src/entry.S");
    println!("cargo:rerun-if-changed=src/kernelvec.S");
    println!("cargo:rerun-if-changed=src/swtch.S");
    println!("cargo:rerun-if-changed=src/trampoline.S");

    // use the `cc` crate to assemble the assembly file and statically link it.
    cc::Build::new()
        .file("src/entry.S")
        .file("src/kernelvec.S")
        .file("src/swtch.S")
        .file("src/trampoline.S")
        .compile("asm");
}
