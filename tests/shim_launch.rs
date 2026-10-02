#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn shim_exe() -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("clevershim-shim");
    path
}

fn write_exec(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn sidecar(path: &Path, target: &Path, manager: &Path) {
    let text = format!(
        "clevershim-sidecar 1\ncommand=tool\npackage_id=Demo.Tool\nscope=user\ntarget={}\nrunner=\nworking_directory={}\npath_prefix=false\nmanager={}\n",
        target.display(),
        target.parent().unwrap().display(),
        manager.display()
    );
    fs::write(path, text).unwrap();
}

#[test]
fn successful_launch_is_silent_even_when_repair_is_noisy() {
    let root = std::env::temp_dir().join(format!("clevershim-launch-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("bin")).unwrap();
    let target = root.join("new/tool.sh");
    write_exec(&target, "#!/bin/sh\nprintf 'from-target'\n");
    let missing = root.join("old/tool.sh");
    let manager = root.join("manager.sh");
    let log = root.join("repair.log");
    write_exec(
        &manager,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nprintf 'noisy-stdout\\n'\nprintf 'noisy-stderr\\n' >&2\ncat > '{}' <<'EOF'\nclevershim-sidecar 1\ncommand=tool\npackage_id=Demo.Tool\nscope=user\ntarget={}\nrunner=\nworking_directory={}\npath_prefix=false\nmanager={}\nEOF\nexit 0\n",
            log.display(),
            root.join("bin/tool.shim").display(),
            target.display(),
            target.parent().unwrap().display(),
            manager.display()
        ),
    );
    let shim = root.join("bin/tool.exe");
    fs::copy(shim_exe(), &shim).unwrap();
    sidecar(&root.join("bin/tool.shim"), &missing, &manager);
    let output = Command::new(&shim).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{:?}", output);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "from-target");
    assert!(
        output.stderr.is_empty(),
        "stderr leaked: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let calls = fs::read_to_string(&log).unwrap().lines().count();
    assert_eq!(calls, 1);
}

#[test]
fn a_second_missing_path_does_not_repair_again() {
    let root = std::env::temp_dir().join(format!("clevershim-once-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("bin")).unwrap();
    let manager = root.join("manager.sh");
    let log = root.join("repair.log");
    let still_missing = root.join("still-missing.sh");
    write_exec(
        &manager,
        &format!(
            "#!/bin/sh\nprintf 'call\\n' >> '{}'\nprintf 'repair-failed-detail\\n' >&2\ncat > '{}' <<'EOF'\nclevershim-sidecar 1\ncommand=tool\npackage_id=Demo.Tool\nscope=user\ntarget={}\nrunner=\nworking_directory={}\npath_prefix=false\nmanager={}\nEOF\nexit 1\n",
            log.display(),
            root.join("bin/tool.shim").display(),
            still_missing.display(),
            root.display(),
            manager.display()
        ),
    );
    let shim = root.join("bin/tool.exe");
    fs::copy(shim_exe(), &shim).unwrap();
    sidecar(&root.join("bin/tool.shim"), &root.join("old.sh"), &manager);
    let output = Command::new(&shim).output().unwrap();
    assert_ne!(output.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("repair-failed-detail"), "{stderr}");
    assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 1);
}

#[test]
fn an_existing_failing_target_is_returned_without_repair() {
    let root = std::env::temp_dir().join(format!("clevershim-exit-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("bin")).unwrap();
    let target = root.join("tool.sh");
    write_exec(&target, "#!/bin/sh\nexit 4\n");
    let manager = root.join("manager.sh");
    let log = root.join("repair.log");
    write_exec(
        &manager,
        &format!("#!/bin/sh\nprintf 'called\\n' >> '{}'\n", log.display()),
    );
    let shim = root.join("bin/tool.exe");
    fs::copy(shim_exe(), &shim).unwrap();
    sidecar(&root.join("bin/tool.shim"), &target, &manager);
    let output = Command::new(&shim).output().unwrap();
    assert_eq!(output.status.code(), Some(4));
    assert!(!log.exists());
}
