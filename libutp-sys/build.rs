extern crate bindgen;
#[macro_use]
extern crate unwrap;
extern crate cc;

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    if !Path::new("curl/.git").exists() {
        let _ = Command::new("git")
            .args(&["submodule", "update", "--init", "--recursive"])
            .status();
    }

    let out_dir = PathBuf::from(unwrap!(env::var("OUT_DIR")));
    compile_libutp(&out_dir);
    println!("cargo:rustc-link-lib=static=utp");
    println!("cargo:rustc-link-search=native=build");
    gen_libutp_rust_bindings(&out_dir);
}

fn compile_libutp(out_dir: &PathBuf) {
    let target = env::var("TARGET").unwrap();
    let windows = target.contains("windows");

    let mut cfg = cc::Build::new();
    cfg.out_dir(out_dir)
        .cpp(true)
        .include("libutp")
        .file("libutp/utp_api.cpp")
        .file("libutp/utp_callbacks.cpp")
        .file("libutp/utp_hash.cpp")
        .file("libutp/utp_internal.cpp")
        .file("libutp/utp_packedsockaddr.cpp")
        .file("libutp/utp_utils.cpp");
    if windows {
        cfg.file("libutp/libutp_inet_ntop.cpp");
    } else {
        cfg.define("POSIX", None);
    }
    cfg.compile("utp");
}

/// Write the bindings to the $OUT_DIR/bindings.rs file.
fn gen_libutp_rust_bindings(out_dir: &PathBuf) {
    let bindings = bindgen::Builder::default()
        .header("utp.h")
        .layout_tests(false)
        .blacklist_type("sockaddr.*")
        .ctypes_prefix("libc")
        .generate()
        .expect("Unable to generate bindings");

    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
