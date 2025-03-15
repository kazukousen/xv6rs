fn main() {
    // Tell the Cargo that if the given file changes, to return the build script.
    println!("cargo:rerun-if-changed=src/usys.S");
    println!("cargo:rerun-if-changed=src/user.ld");

    // Print out the environment variables for debugging
    println!("OUT_DIR = {:?}", std::env::var_os("OUT_DIR"));
    println!("OPT_LEVEL = {:?}", std::env::var_os("OPT_LEVEL"));
    println!("TARGET = {:?}", std::env::var_os("TARGET"));
    println!("HOST = {:?}", std::env::var_os("HOST"));

    // use the `cc` crate to assemble the assembly file and statically link it.
    cc::Build::new()
        .file("src/usys.S")
        .flag("-march=rv64imac_zicsr_zifencei") // Add zicsr and zifencei extensions for consistency
        .compile("asm");
}
