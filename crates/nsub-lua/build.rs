fn main() {
    // LPeg is required by MoonScript (and used directly by some scripts). It is compiled against
    // the LuaJIT headers vendored in native/include (same version as the luajit-src crate mlua uses).
    cc::Build::new().file("native/lpeg.c").include("native/include").include("native").warnings(false).compile("lpeg");
    println!("cargo:rerun-if-changed=native/lpeg.c");
}
