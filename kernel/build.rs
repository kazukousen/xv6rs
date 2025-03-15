fn main() {
    // Tell the Cargo that if the given file changes, to return the build script.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/entry.S");
    println!("cargo:rerun-if-changed=src/kernelvec.S");
    println!("cargo:rerun-if-changed=src/swtch.S");
    println!("cargo:rerun-if-changed=src/trampoline.S");

    // Print out the environment variables for debugging
    println!("OUT_DIR = {:?}", std::env::var_os("OUT_DIR"));
    println!("OPT_LEVEL = {:?}", std::env::var_os("OPT_LEVEL"));
    println!("TARGET = {:?}", std::env::var_os("TARGET"));
    println!("HOST = {:?}", std::env::var_os("HOST"));

    // use the `cc` crate to assemble the assembly file and statically link it.
    cc::Build::new()
        .file("src/entry.S")
        .file("src/kernelvec.S")
        .file("src/swtch.S")
        .file("src/trampoline.S")
        .flag("-march=rv64imac_zicsr_zifencei") // Add zicsr and zifencei extensions
        .compile("asm");
}
