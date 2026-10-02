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

pub fn path_is_inside(path: &Path, dir: &Path) -> bool {
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
    if paths_equal(target, shim_exe) || paths_equal(target, manager) {
        return true;
    }
    if path_is_inside(target, bin_dir) {
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
}
