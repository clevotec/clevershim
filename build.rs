#[cfg(windows)]
fn set_windows_version_info() {
    if std::env::var("CARGO_CFG_TARGET_OS").ok().as_deref() == Some("windows") {
        winres::WindowsResource::new()
            .set("FileDescription", "CleverShim installer")
            .set("ProductName", "CleverShim")
            .set("CompanyName", "Clevotec")
            .compile()
            .expect("failed to compile Windows version information");
    }
}

#[cfg(not(windows))]
fn set_windows_version_info() {}

fn main() {
    set_windows_version_info();
    println!("cargo:rustc-check-cfg=cfg(embedded_shim)");
    println!("cargo:rerun-if-changed=assets/clevershim-shim.exe");
    let stub = std::path::Path::new("assets/clevershim-shim.exe");
    let present = std::fs::metadata(stub)
        .map(|meta| meta.len() > 0)
        .unwrap_or(false);
    if present {
        println!("cargo:rustc-cfg=embedded_shim");
    }
}
