#[cfg(not(target_os = "freebsd"))]
fn main() {}

#[cfg(target_os = "freebsd")]
fn main() {
    println!("cargo:rustc-link-search=/usr/local/lib");
}
