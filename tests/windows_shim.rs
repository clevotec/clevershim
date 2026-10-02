#![cfg(windows)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn packaged_bin(name: &str) -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push(format!("{name}.exe"));
    path
}

fn manager_exe() -> PathBuf {
    packaged_bin("clevershim")
}

fn stub_exe() -> PathBuf {
    packaged_bin("clevershim-shim")
}

fn write_sidecar(path: &Path, target: &Path, manager: &Path, command: &str, package: &str) {
    let text = format!(
        "clevershim-sidecar 1\ncommand={command}\npackage_id={package}\nscope=user\ntarget={}\nrunner=\nworking_directory={}\npath_prefix=false\nmanager={}\n",
        target.display(),
        target.parent().unwrap().display(),
        manager.display()
    );
    fs::write(path, text).unwrap();
}

#[test]
fn shim_repairs_and_loads_a_sibling_dll() {
    let root = std::env::temp_dir().join(format!("clevershim-dll-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let package = root.join("local/Microsoft/WinGet/Packages/Demo.Tool_2");
    fs::create_dir_all(&package).unwrap();
    let lib_rs = root.join("fixture_lib.rs");
    let main_rs = root.join("fixture_main.rs");
    fs::write(
        &lib_rs,
        "#[no_mangle]\npub extern \"system\" fn clevershim_fixture_ping() -> i32 { 42 }\n",
    )
    .unwrap();
    fs::write(
        &main_rs,
        r#"
#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut core::ffi::c_void;
    fn GetProcAddress(module: *mut core::ffi::c_void, name: *const u8) -> *mut core::ffi::c_void;
}
fn main() {
    let mut dll = std::env::current_exe().unwrap();
    dll.pop();
    dll.push("clevershim_fixture.dll");
    let wide: Vec<u16> = dll.as_os_str().to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let module = LoadLibraryW(wide.as_ptr());
        if module.is_null() { std::process::exit(3); }
        let symbol = GetProcAddress(module, b"clevershim_fixture_ping\0".as_ptr());
        if symbol.is_null() { std::process::exit(4); }
        let ping: extern "system" fn() -> i32 = std::mem::transmute(symbol);
        if ping() == 42 {
            println!("dll-ok");
            std::process::exit(0);
        }
    }
    std::process::exit(2);
}
"#,
    )
    .unwrap();
    let dll = package.join("clevershim_fixture.dll");
    let exe = package.join("demo.exe");
    let status = Command::new("rustc")
        .args(["--crate-type", "cdylib", "-o"])
        .arg(&dll)
        .arg(&lib_rs)
        .status()
        .expect("rustc");
    assert!(status.success());
    let status = Command::new("rustc")
        .arg("-o")
        .arg(&exe)
        .arg(&main_rs)
        .status()
        .expect("rustc");
    assert!(status.success(), "fixture exe failed to compile");

    let manager = manager_exe();
    let stub = stub_exe();
    let shim = root.join("local/Clevotec/CleverShim/bin/demo.exe");
    fs::create_dir_all(shim.parent().unwrap()).unwrap();
    fs::copy(&stub, &shim).unwrap();
    write_sidecar(
        &root.join("local/Clevotec/CleverShim/bin/demo.shim"),
        &root.join("deleted/demo.exe"),
        &manager,
        "demo",
        "Demo.Tool",
    );
    fs::create_dir_all(root.join("local/clevershim")).unwrap();
    fs::write(
        root.join("local/clevershim/packages.yaml"),
        "packages:\n  - id: Demo.Tool\n    version: '1'\n    commands: [demo]\n    archive_binaries_depend_on_path: false\n",
    )
    .unwrap();
    let winget = root.join("winget.cmd");
    fs::write(
        &winget,
        "@echo off\r\necho Name Id Version Source\r\necho Demo Demo.Tool 1.2.3 winget\r\nexit /b 0\r\n",
    )
    .unwrap();
    let output = Command::new(&shim)
        .env("CLEVERSHIM_LOCALAPPDATA", root.join("local"))
        .env("CLEVERSHIM_PROGRAMDATA", root.join("programdata"))
        .env("CLEVERSHIM_PROGRAMFILES", root.join("programfiles"))
        .env("CLEVERSHIM_WINGET", &winget)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout {} stderr {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "dll-ok");
    assert!(output.stderr.is_empty());
}

#[test]
fn missing_after_repair_does_not_call_repair_twice() {
    let root = std::env::temp_dir().join(format!("clevershim-win-once-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let log = root.join("count.txt");
    fs::create_dir_all(&root).unwrap();
    let manager_rs = root.join("fake.rs");
    fs::write(
        &manager_rs,
        r#"fn main() {
    let log = std::env::var("CLEVERSHIM_COUNT").unwrap();
    let shim = std::env::var("CLEVERSHIM_SIDECAR").unwrap();
    let manager = std::env::var("CLEVERSHIM_FAKE").unwrap();
    let prior = std::fs::read_to_string(&log).unwrap_or_default();
    std::fs::write(&log, format!("{prior}x")).unwrap();
    let body = format!("clevershim-sidecar 1\ncommand=demo\npackage_id=Demo.Tool\nscope=user\ntarget=C:\\missing\\demo.exe\nrunner=\nworking_directory=C:\\\npath_prefix=false\nmanager={manager}\n");
    std::fs::write(shim, body).unwrap();
    eprintln!("repair-failed-detail");
    std::process::exit(1);
}
"#,
    )
    .unwrap();
    let fake = root.join("fake.exe");
    let status = Command::new("rustc")
        .arg("-o")
        .arg(&fake)
        .arg(&manager_rs)
        .status()
        .unwrap();
    assert!(status.success());
    let stub = stub_exe();
    let shim = root.join("bin/demo.exe");
    fs::create_dir_all(shim.parent().unwrap()).unwrap();
    fs::copy(&stub, &shim).unwrap();
    fs::write(
        root.join("bin/demo.shim"),
        format!(
            "clevershim-sidecar 1\ncommand=demo\npackage_id=Demo.Tool\nscope=user\ntarget=C:\\gone\\demo.exe\nrunner=\nworking_directory=C:\\\npath_prefix=false\nmanager={}\n",
            fake.display()
        ),
    )
    .unwrap();
    let output = Command::new(&shim)
        .env("CLEVERSHIM_COUNT", &log)
        .env("CLEVERSHIM_SIDECAR", root.join("bin/demo.shim"))
        .env("CLEVERSHIM_FAKE", &fake)
        .output()
        .unwrap();
    assert_ne!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stderr).contains("repair-failed-detail"));
    assert_eq!(fs::read_to_string(&log).unwrap(), "x");
}

#[test]
fn gone_package_drops_only_that_scope() {
    let root = std::env::temp_dir().join(format!("clevershim-win-scope-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let local = root.join("local");
    let program_data = root.join("programdata");
    let program_files = root.join("programfiles");
    fs::create_dir_all(local.join("Microsoft/WinGet/Packages")).unwrap();
    let machine_exe = program_files.join("WinGet/Packages/Other.Tool_1/other.exe");
    fs::create_dir_all(machine_exe.parent().unwrap()).unwrap();
    fs::write(&machine_exe, b"other").unwrap();
    let user_bin = local.join("Clevotec/CleverShim/bin");
    let machine_bin = program_data.join("Clevotec/CleverShim/bin");
    fs::create_dir_all(&user_bin).unwrap();
    fs::create_dir_all(&machine_bin).unwrap();
    fs::write(user_bin.join("demo.exe"), b"shim").unwrap();
    fs::write(
        user_bin.join("demo.shim"),
        "clevershim-sidecar 1\ncommand=demo\npackage_id=Demo.Tool\nscope=user\ntarget=C:\\gone\\demo.exe\nrunner=\nworking_directory=C:\\\npath_prefix=false\nmanager=C:\\clevershim.exe\n",
    )
    .unwrap();
    fs::write(machine_bin.join("other.exe"), b"shim").unwrap();
    fs::write(
        machine_bin.join("other.shim"),
        format!(
            "clevershim-sidecar 1\ncommand=other\npackage_id=Other.Tool\nscope=machine\ntarget={}\nrunner=\nworking_directory={}\npath_prefix=false\nmanager=C:\\clevershim.exe\n",
            machine_exe.display(),
            machine_exe.parent().unwrap().display()
        ),
    )
    .unwrap();
    fs::create_dir_all(local.join("clevershim")).unwrap();
    fs::write(
        local.join("clevershim/packages.yaml"),
        "packages:\n  - id: Demo.Tool\n    version: '1'\n    commands: [demo]\n  - id: Other.Tool\n    version: '1'\n    commands: [other]\n",
    )
    .unwrap();
    let clevershim = manager_exe();
    let status = Command::new(&clevershim)
        .arg("sync")
        .env("CLEVERSHIM_LOCALAPPDATA", &local)
        .env("CLEVERSHIM_PROGRAMDATA", &program_data)
        .env("CLEVERSHIM_PROGRAMFILES", &program_files)
        .env("CLEVERSHIM_CATALOG", local.join("clevershim/packages.yaml"))
        .status()
        .unwrap();
    assert!(status.success());
    assert!(!user_bin.join("demo.shim").exists());
    assert!(machine_bin.join("other.shim").is_file());
}
