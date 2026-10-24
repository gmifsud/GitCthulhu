fn main() {
    println!("cargo:rerun-if-changed=assets/git_cthulhu.ico");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/git_cthulhu.ico");
    res.compile().expect("embed the Cthulhu icon");
}
