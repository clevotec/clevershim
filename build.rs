fn main() {
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
