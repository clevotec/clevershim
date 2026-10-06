use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Sidecar written next to a shim exe. The stub and the manager share this text format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sidecar {
    pub command: String,
    pub package_id: String,
    pub scope: String,
    pub target: String,
    pub runner: String,
    pub runner_args: Vec<String>,
    pub working_directory: String,
    pub path_prefix: bool,
    pub manager: String,
}

impl Default for Sidecar {
    fn default() -> Self {
        Self {
            command: String::new(),
            package_id: String::new(),
            scope: String::new(),
            target: String::new(),
            runner: String::new(),
            runner_args: Vec::new(),
            working_directory: String::new(),
            path_prefix: false,
            manager: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchPlan {
    Launch,
    RepairOnce,
    GiveUp,
}

/// Repair runs only when the recorded file is absent, and only once per shim process.
pub fn launch_plan(target_exists: bool, already_repaired: bool) -> LaunchPlan {
    if target_exists {
        LaunchPlan::Launch
    } else if already_repaired {
        LaunchPlan::GiveUp
    } else {
        LaunchPlan::RepairOnce
    }
}

pub fn sidecar_path_for_exe(exe: &Path) -> PathBuf {
    let mut path = exe.to_path_buf();
    path.set_extension("shim");
    path
}

pub fn load(path: &Path) -> io::Result<Sidecar> {
    let text = fs::read_to_string(path)?;
    parse(&text).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

pub fn store(path: &Path, sidecar: &Sidecar) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(path)?;
    file.write_all(render(sidecar).as_bytes())?;
    Ok(())
}

pub fn parse(text: &str) -> Result<Sidecar, String> {
    let mut lines = text.lines().filter(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with('#')
    });
    let header = lines.next().unwrap_or("").trim();
    if !header.starts_with("clevershim-sidecar") {
        return Err("sidecar is missing the clevershim-sidecar header".into());
    }
    let mut sidecar = Sidecar::default();
    for line in lines {
        let line = line.trim();
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("invalid sidecar line: {line}"));
        };
        match key {
            "command" => sidecar.command = value.to_string(),
            "package_id" => sidecar.package_id = value.to_string(),
            "scope" => sidecar.scope = value.to_string(),
            "target" => sidecar.target = value.to_string(),
            "runner" => sidecar.runner = value.to_string(),
            "runner_arg" => sidecar.runner_args.push(value.to_string()),
            "working_directory" => sidecar.working_directory = value.to_string(),
            "path_prefix" => sidecar.path_prefix = matches!(value, "1" | "true" | "yes"),
            "manager" => sidecar.manager = value.to_string(),
            _ => {}
        }
    }
    Ok(sidecar)
}

pub fn render(sidecar: &Sidecar) -> String {
    let mut out = String::from("clevershim-sidecar 1\n");
    push(&mut out, "command", &sidecar.command);
    push(&mut out, "package_id", &sidecar.package_id);
    push(&mut out, "scope", &sidecar.scope);
    push(&mut out, "target", &sidecar.target);
    push(&mut out, "runner", &sidecar.runner);
    for arg in &sidecar.runner_args {
        out.push_str("runner_arg=");
        out.push_str(arg);
        out.push('\n');
    }
    push(&mut out, "working_directory", &sidecar.working_directory);
    push(
        &mut out,
        "path_prefix",
        if sidecar.path_prefix { "true" } else { "false" },
    );
    push(&mut out, "manager", &sidecar.manager);
    out
}

fn push(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push('=');
    out.push_str(value);
    out.push('\n');
}

pub fn paths_equal(left: &Path, right: &Path) -> bool {
    paths_equal_resolved(&resolve_for_compare(left), &resolve_for_compare(right))
}

pub(crate) fn paths_equal_resolved(left: &Path, right: &Path) -> bool {
    let left = normalize_for_compare(left);
    let right = normalize_for_compare(right);
    if cfg!(windows) {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    }
}

pub fn normalize_for_compare(path: &Path) -> String {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                parts.pop();
            }
            #[cfg(windows)]
            std::path::Component::Prefix(prefix) => {
                use std::path::Prefix;
                parts.push(match prefix.kind() {
                    Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => {
                        format!("{}:", drive as char)
                    }
                    Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => {
                        format!(
                            r"\\{}\{}",
                            server.to_string_lossy(),
                            share.to_string_lossy()
                        )
                    }
                    _ => prefix.as_os_str().to_string_lossy().into_owned(),
                });
            }
            other => parts.push(other.as_os_str().to_string_lossy().into_owned()),
        }
    }
    let joined = parts.join(std::path::MAIN_SEPARATOR_STR);
    let trimmed = joined.trim_end_matches(['\\', '/']).to_string();
    if cfg!(windows) {
        trimmed.to_ascii_lowercase()
    } else {
        trimmed
    }
}

/// Prefer the OS-resolved path so Windows short names and junctions match canonicalized targets.
pub fn resolve_for_compare(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

pub fn path_is_inside(path: &Path, dir: &Path) -> bool {
    path_is_inside_resolved(&resolve_for_compare(path), &resolve_for_compare(dir))
}

fn path_is_inside_resolved(path: &Path, dir: &Path) -> bool {
    let path = normalize_for_compare(path);
    let dir = normalize_for_compare(dir);
    if path == dir {
        return true;
    }
    let sep = std::path::MAIN_SEPARATOR;
    path.starts_with(&format!("{dir}{sep}"))
}

/// Repair will not point a shim at itself, at another file in its bin, or at clevershim.
pub fn is_forbidden_target(target: &Path, shim_exe: &Path, bin_dir: &Path, manager: &Path) -> bool {
    let target = resolve_for_compare(target);
    let shim_exe = resolve_for_compare(shim_exe);
    let bin_dir = resolve_for_compare(bin_dir);
    let manager = resolve_for_compare(manager);
    is_forbidden_target_resolved(&target, &shim_exe, &bin_dir, &manager)
}

/// Compare already-resolved paths without filesystem I/O during candidate selection.
pub(crate) fn is_forbidden_target_resolved(
    target: &Path,
    shim_exe: &Path,
    bin_dir: &Path,
    manager: &Path,
) -> bool {
    if paths_equal_resolved(target, shim_exe) || paths_equal_resolved(target, manager) {
        return true;
    }
    if path_is_inside_resolved(target, bin_dir) {
        return true;
    }
    match target.file_name().and_then(|name| name.to_str()) {
        Some(name) => {
            let lower = name.to_ascii_lowercase();
            matches!(
                lower.as_str(),
                "clevershim.exe" | "clevershim-shim.exe" | "clevershim" | "clevershim-shim"
            )
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_round_trip_keeps_runner_args() {
        let sidecar = Sidecar {
            command: "ffmpeg".into(),
            package_id: "Gyan.FFmpeg".into(),
            scope: "user".into(),
            target: r"C:\Tools\ffmpeg.bat".into(),
            runner: r"C:\Windows\System32\cmd.exe".into(),
            runner_args: vec!["/c".into(), r"C:\Tools\ffmpeg.bat".into()],
            working_directory: r"C:\Tools".into(),
            path_prefix: true,
            manager: r"C:\Users\A\Clevotec\CleverShim\clevershim.exe".into(),
        };
        let parsed = parse(&render(&sidecar)).unwrap();
        assert_eq!(parsed, sidecar);
    }

    #[test]
    fn launch_plan_repairs_once() {
        assert_eq!(launch_plan(true, false), LaunchPlan::Launch);
        assert_eq!(launch_plan(false, false), LaunchPlan::RepairOnce);
        assert_eq!(launch_plan(true, true), LaunchPlan::Launch);
        assert_eq!(launch_plan(false, true), LaunchPlan::GiveUp);
    }

    #[test]
    fn refuses_self_and_bin_targets() {
        let shim = Path::new("/tmp/clevershim/bin/ffmpeg.exe");
        let bin = Path::new("/tmp/clevershim/bin");
        let manager = Path::new("/tmp/clevershim/clevershim.exe");
        assert!(is_forbidden_target(shim, shim, bin, manager));
        assert!(is_forbidden_target(
            Path::new("/tmp/clevershim/bin/ffprobe.exe"),
            shim,
            bin,
            manager
        ));
        assert!(is_forbidden_target(
            Path::new("/opt/clevershim.exe"),
            shim,
            bin,
            manager
        ));
        assert!(!is_forbidden_target(
            Path::new("/opt/winget/ffmpeg.exe"),
            shim,
            bin,
            manager
        ));
    }

    #[cfg(windows)]
    #[test]
    fn windows_verbatim_paths_cannot_bypass_bin_exclusion() {
        for (target, bin) in [
            (r"\\?\C:\Tools\bin\tool.exe", r"C:\Tools\bin"),
            (r"\\?\UNC\server\share\bin\tool.exe", r"\\server\share\bin"),
        ] {
            let bin = Path::new(bin);
            let shim = bin.join("other.exe");
            let manager = bin.with_file_name("clevershim.exe");
            assert!(is_forbidden_target(Path::new(target), &shim, bin, &manager));
        }
        assert!(!path_is_inside(
            Path::new(r"\\?\C:\Tools\bin-other\tool.exe"),
            Path::new(r"C:\Tools\bin")
        ));
    }

    #[cfg(windows)]
    #[test]
    fn short_path_aliases_cannot_bypass_bin_exclusion() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "clevershim-sidecar-short-{}-{nanos}",
            std::process::id()
        ));
        let bin = root.join("bin");
        let shim = bin.join("ffmpeg.exe");
        let manager = root.join("clevershim.exe");
        let decoy = bin.join("ffprobe.exe");
        fs::create_dir_all(&bin).unwrap();
        fs::write(&shim, b"shim").unwrap();
        fs::write(&manager, b"mgr").unwrap();
        fs::write(&decoy, b"decoy").unwrap();

        let short_bin = short_path(&bin).unwrap_or_else(|| bin.clone());
        let canonical_bin = fs::canonicalize(&bin).unwrap();
        if normalize_for_compare(&short_bin) == normalize_for_compare(&canonical_bin) {
            eprintln!("Skipping short-path test: this volume does not provide an 8.3 alias");
            fs::remove_dir_all(&root).unwrap();
            return;
        }
        let short_shim = short_path(&shim).unwrap_or_else(|| shim.clone());
        let short_manager = short_path(&manager).unwrap_or_else(|| manager.clone());
        let canonical_decoy = fs::canonicalize(&decoy).unwrap();

        assert!(paths_equal(&shim, &short_shim));
        assert!(path_is_inside(&canonical_decoy, &short_bin));
        assert!(is_forbidden_target(
            &canonical_decoy,
            &short_shim,
            &short_bin,
            &short_manager
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    fn short_path(path: &Path) -> Option<PathBuf> {
        use std::ffi::OsString;
        use std::os::windows::ffi::{OsStrExt, OsStringExt};

        #[link(name = "kernel32")]
        extern "system" {
            fn GetShortPathNameW(
                long_path: *const u16,
                short_path: *mut u16,
                buffer_len: u32,
            ) -> u32;
        }

        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let needed = unsafe { GetShortPathNameW(wide.as_ptr(), std::ptr::null_mut(), 0) };
        if needed == 0 {
            return None;
        }
        let mut buf = vec![0u16; needed as usize];
        let written = unsafe { GetShortPathNameW(wide.as_ptr(), buf.as_mut_ptr(), needed) };
        if written == 0 || written >= needed {
            return None;
        }
        Some(PathBuf::from(OsString::from_wide(&buf[..written as usize])))
    }
}
