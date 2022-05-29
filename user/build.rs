fn main() {
    // Tell the Cargo that if the given file changes, to return the build script.
    println!("cargo:rerun-if-changed=src/usys.S");
    println!("cargo:rerun-if-changed=src/user.ld");

    // use the `cc` crate to assemble the assembly file and statically link it.
    cc::Build::new().file("src/usys.S").compile("asm");
}
