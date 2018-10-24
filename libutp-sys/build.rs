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

    let target = env::var("TARGET").unwrap();
    let target_is_windows = target.contains("windows");

    let out_dir = PathBuf::from(unwrap!(env::var("OUT_DIR")));
    compile_libutp(&out_dir, target_is_windows);
    println!("cargo:rustc-link-lib=static=utp");
    println!("cargo:rustc-link-search=native=build");
    gen_libutp_rust_bindings(&out_dir, target_is_windows);
}

fn compile_libutp(out_dir: &PathBuf, target_is_windows: bool) {
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
    if target_is_windows {
        cfg.define("WIN32", None)
            .file("libutp/libutp_inet_ntop.cpp");
    } else {
        cfg.define("POSIX", None);
    }
    cfg.compile("utp");
}

/// Write the bindings to the $OUT_DIR/bindings.rs file.
fn gen_libutp_rust_bindings(out_dir: &PathBuf, target_is_windows: bool) {
    let header = if target_is_windows {
        "utp_win.h"
    } else {
        "utp_posix.h"
    };
    let bindings = bindgen::Builder::default()
        .header(header)
        .layout_tests(false)
        .blacklist_type("sockaddr.*")
        .ctypes_prefix("libc")
        .generate()
        .expect("Unable to generate bindings");

    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
