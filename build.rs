fn main() {
    println!("cargo:rustc-link-search=native=/Users/nikita/dev/libutp/");
    println!("cargo:rustc-link-lib=utp");
}
