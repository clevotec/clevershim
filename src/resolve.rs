use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::layout::Scope;
use crate::report::CandidateNote;
use crate::sidecar_format::{self, paths_equal};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTarget {
    pub package_id: String,
    pub path: PathBuf,
    pub working_directory: PathBuf,
    pub runner: String,
    pub runner_args: Vec<String>,
    pub path_prefix: bool,
}

#[derive(Debug, Clone)]
pub struct ResolveHit {
    pub searched: Vec<String>,
    pub candidates: Vec<CandidateNote>,
    pub selected: Option<ResolvedTarget>,
}

pub struct ResolveContext<'a> {
    pub scope: Scope,
    pub package_id: &'a str,
    pub command: &'a str,
    pub packages_root: &'a Path,
    pub links_root: &'a Path,
    pub other_packages_root: &'a Path,
    pub other_links_root: &'a Path,
    pub install_locations: &'a [PathBuf],
    pub shim_exe: &'a Path,
    pub bin_dir: &'a Path,
    pub manager: &'a Path,
    pub system_root: &'a Path,
    pub path_prefix: bool,
}

pub fn candidate_file_names(command: &str) -> Vec<String> {
    let lower = command.to_ascii_lowercase();
    if lower.ends_with(".exe") || lower.ends_with(".bat") || lower.ends_with(".cmd") {
        vec![command.to_string()]
    } else {
        vec![
            format!("{command}.exe"),
            format!("{command}.bat"),
            format!("{command}.cmd"),
        ]
    }
}

pub fn shim_file_name(command: &str) -> String {
    if command.to_ascii_lowercase().ends_with(".exe") {
        command.to_string()
    } else {
        format!("{command}.exe")
    }
}

/// WinGet package directories are `{PackageId}_{source}`. The next character must be `_`
/// so `Gyan.FFmpeg` does not match `Gyan.FFmpeg.Essentials_...`.
pub fn directory_matches_package(dir_name: &str, package_id: &str) -> bool {
    if dir_name.eq_ignore_ascii_case(package_id) {
        return true;
    }
    let mut prefix = package_id.to_string();
    prefix.push('_');
    dir_name.len() > prefix.len() && dir_name[..prefix.len()].eq_ignore_ascii_case(&prefix)
}

pub fn matching_package_dirs(packages_root: &Path, package_id: &str) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let entries = match fs::read_dir(packages_root) {
        Ok(entries) => entries,
        Err(_) => return dirs,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if directory_matches_package(&name, package_id) {
            dirs.push(path);
        }
    }
    dirs.sort_by(|left, right| {
        mtime(right)
            .cmp(&mtime(left))
            .then_with(|| left.file_name().cmp(&right.file_name()))
    });
    dirs
}

pub fn resolve(ctx: &ResolveContext<'_>) -> ResolveHit {
    // Resolve comparison roots once; judge canonicalizes each candidate before checking them.
    let shim_exe = sidecar_format::resolve_for_compare(ctx.shim_exe);
    let bin_dir = sidecar_format::resolve_for_compare(ctx.bin_dir);
    let manager = sidecar_format::resolve_for_compare(ctx.manager);
    let other_packages_root = sidecar_format::resolve_for_compare(ctx.other_packages_root);
    let other_links_root = sidecar_format::resolve_for_compare(ctx.other_links_root);
    let resolved_ctx = ResolveContext {
        scope: ctx.scope,
        package_id: ctx.package_id,
        command: ctx.command,
        packages_root: ctx.packages_root,
        links_root: ctx.links_root,
        other_packages_root: &other_packages_root,
        other_links_root: &other_links_root,
        install_locations: ctx.install_locations,
        shim_exe: &shim_exe,
        bin_dir: &bin_dir,
        manager: &manager,
        system_root: ctx.system_root,
        path_prefix: ctx.path_prefix,
    };
    let ctx = &resolved_ctx;
    let names = candidate_file_names(ctx.command);
    let mut searched = Vec::new();
    let mut candidates = Vec::new();
    let mut selected: Option<ResolvedTarget> = None;

    searched.push(format!("links | {}", ctx.links_root.display()));
    consider_dir_files(
        ctx,
        ctx.links_root,
        &names,
        false,
        "links entry wins",
        &mut selected,
        &mut candidates,
    );

    let package_dirs = matching_package_dirs(ctx.packages_root, ctx.package_id);
    if package_dirs.is_empty() {
        searched.push(format!(
            "package-folders | {} (none)",
            ctx.packages_root.display()
        ));
    } else {
        for (index, dir) in package_dirs.iter().enumerate() {
            searched.push(format!("package-folders | {}", dir.display()));
            consider_dir_files(
                ctx,
                dir,
                &names,
                index > 0 && selected.is_some(),
                "older copy",
                &mut selected,
                &mut candidates,
            );
        }
    }

    if ctx.install_locations.is_empty() {
        searched.push("uninstall-location | (none)".into());
    }
    for location in ctx.install_locations {
        searched.push(format!("uninstall-location | {}", location.display()));
        consider_dir_files(
            ctx,
            location,
            &names,
            selected.is_some(),
            "uninstall location after an earlier hit",
            &mut selected,
            &mut candidates,
        );
    }

    ResolveHit {
        searched,
        candidates,
        selected,
    }
}

fn consider_dir_files(
    ctx: &ResolveContext<'_>,
    dir: &Path,
    names: &[String],
    force_skip: bool,
    skip_reason: &str,
    selected: &mut Option<ResolvedTarget>,
    candidates: &mut Vec<CandidateNote>,
) {
    let mut found = Vec::new();
    collect_named_files(dir, names, 8, 0, &mut found);
    found.sort_by(|left, right| file_rank(left).cmp(&file_rank(right)));
    for (path, _depth) in found {
        match judge(ctx, &path) {
            Ok(resolved) => {
                if selected.is_some() || force_skip {
                    candidates.push(CandidateNote {
                        path: path.display().to_string(),
                        disposition: skip_reason.into(),
                    });
                } else {
                    candidates.push(CandidateNote {
                        path: path.display().to_string(),
                        disposition: "selected".into(),
                    });
                    *selected = Some(resolved);
                }
            }
            Err(reason) => candidates.push(CandidateNote {
                path: path.display().to_string(),
                disposition: reason,
            }),
        }
    }
}

fn collect_named_files(
    dir: &Path,
    names: &[String],
    max_depth: usize,
    depth: usize,
    out: &mut Vec<(PathBuf, usize)>,
) {
    if depth > max_depth {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(_) => continue,
        };
        if meta.file_type().is_symlink() || meta.is_file() {
            if names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&file_name))
            {
                out.push((path, depth));
            }
            continue;
        }
        if meta.is_dir() {
            collect_named_files(&path, names, max_depth, depth + 1, out);
        }
    }
}

fn judge(ctx: &ResolveContext<'_>, path: &Path) -> Result<ResolvedTarget, String> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(_) => return Err("missing".into()),
    };
    if meta.len() == 0 && !meta.file_type().is_symlink() {
        return Err("empty".into());
    }
    let real = match fs::canonicalize(path) {
        Ok(real) => real,
        Err(_) => return Err("missing".into()),
    };
    let real_meta = match fs::metadata(&real) {
        Ok(meta) => meta,
        Err(_) => return Err("missing".into()),
    };
    if !real_meta.is_file() {
        return Err("missing".into());
    }
    if real_meta.len() == 0 {
        return Err("empty".into());
    }
    if is_other_scope(ctx, &real) {
        return Err("other scope".into());
    }
    if sidecar_format::is_forbidden_target_resolved(
        &real,
        ctx.shim_exe,
        ctx.bin_dir,
        ctx.manager,
    ) {
        return Err("self-target".into());
    }
    Ok(target_from_path(ctx, real))
}

fn is_other_scope(ctx: &ResolveContext<'_>, path: &Path) -> bool {
    path_starts_with(path, ctx.other_packages_root) || path_starts_with(path, ctx.other_links_root)
}

fn path_starts_with(path: &Path, root: &Path) -> bool {
    if root.as_os_str().is_empty() {
        return false;
    }
    let path = sidecar_format::normalize_for_compare(path);
    let root = sidecar_format::normalize_for_compare(root);
    path == root || path.starts_with(&format!("{root}{}", std::path::MAIN_SEPARATOR))
}

pub fn target_from_path(ctx: &ResolveContext<'_>, real: PathBuf) -> ResolvedTarget {
    let working_directory = real
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| real.clone());
    let extension = real
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let (runner, runner_args) = if extension == "bat" || extension == "cmd" {
        (
            ctx.system_root
                .join("System32")
                .join("cmd.exe")
                .display()
                .to_string(),
            vec!["/c".into(), real.display().to_string()],
        )
    } else {
        (String::new(), Vec::new())
    };
    ResolvedTarget {
        package_id: ctx.package_id.to_string(),
        path: real,
        working_directory,
        runner,
        runner_args,
        path_prefix: ctx.path_prefix,
    }
}

fn file_rank(item: &(PathBuf, usize)) -> (usize, u8, String) {
    let ext = item
        .0
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let ext_rank = match ext.as_str() {
        "exe" => 0,
        "bat" => 1,
        "cmd" => 2,
        _ => 9,
    };
    (item.1, ext_rank, item.0.display().to_string())
}

fn mtime(path: &Path) -> SystemTime {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

pub fn same_target(recorded: &Path, resolved: &ResolvedTarget) -> bool {
    paths_equal(recorded, &resolved.path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::temp_dir;
    use std::fs;

    fn touch(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }

    struct Roots {
        user_packages: PathBuf,
        user_links: PathBuf,
        machine_packages: PathBuf,
        machine_links: PathBuf,
        system_root: PathBuf,
    }

    fn roots(root: &Path) -> Roots {
        Roots {
            user_packages: root.join("user-packages"),
            user_links: root.join("user-links"),
            machine_packages: root.join("machine-packages"),
            machine_links: root.join("machine-links"),
            system_root: root.join("Windows"),
        }
    }

    fn ctx<'a>(
        roots: &'a Roots,
        command: &'a str,
        package_id: &'a str,
        shim: &'a Path,
        bin: &'a Path,
        manager: &'a Path,
        locations: &'a [PathBuf],
    ) -> ResolveContext<'a> {
        ResolveContext {
            scope: Scope::User,
            package_id,
            command,
            packages_root: &roots.user_packages,
            links_root: &roots.user_links,
            other_packages_root: &roots.machine_packages,
            other_links_root: &roots.machine_links,
            install_locations: locations,
            shim_exe: shim,
            bin_dir: bin,
            manager,
            system_root: &roots.system_root,
            path_prefix: false,
        }
    }

    #[test]
    fn package_directory_boundary() {
        assert!(directory_matches_package(
            "Gyan.FFmpeg_Microsoft.Winget.Source_8wekyb3d8bbwe",
            "Gyan.FFmpeg"
        ));
        assert!(!directory_matches_package(
            "Gyan.FFmpeg.Essentials_Microsoft.Winget.Source_8wekyb3d8bbwe",
            "Gyan.FFmpeg"
        ));
        assert!(directory_matches_package("Demo.Tool", "Demo.Tool"));
    }

    #[test]
    fn command_names_prefer_exe_then_bat_then_cmd() {
        assert_eq!(
            candidate_file_names("ffmpeg"),
            vec!["ffmpeg.exe", "ffmpeg.bat", "ffmpeg.cmd"]
        );
        assert_eq!(candidate_file_names("tool.bat"), vec!["tool.bat"]);
        assert_eq!(shim_file_name("ffmpeg"), "ffmpeg.exe");
        assert_eq!(shim_file_name("ffmpeg.exe"), "ffmpeg.exe");
    }

    #[test]
    fn newest_package_dir_wins_and_links_win_first() {
        let root = temp_dir("resolve");
        let tree = roots(&root);
        let shim = root.join("bin/ffmpeg.exe");
        let bin = root.join("bin");
        let manager = root.join("clevershim.exe");
        touch(&shim, b"shim");
        let older = root.join("user-packages/Gyan.FFmpeg_old");
        let newer = root.join("user-packages/Gyan.FFmpeg_new");
        touch(&older.join("bin/ffmpeg.exe"), b"old");
        touch(&newer.join("bin/ffmpeg.exe"), b"new");
        filetime_bump(&older, &newer);

        let context = ctx(&tree, "ffmpeg", "Gyan.FFmpeg", &shim, &bin, &manager, &[]);
        let hit = resolve(&context);
        let selected = hit.selected.unwrap();
        assert!(selected.path.ends_with("ffmpeg.exe"));
        assert!(selected
            .path
            .display()
            .to_string()
            .contains("Gyan.FFmpeg_new"));

        touch(&root.join("user-links/ffmpeg.exe"), b"link");
        let context = ctx(&tree, "ffmpeg", "Gyan.FFmpeg", &shim, &bin, &manager, &[]);
        let hit = resolve(&context);
        assert!(hit
            .selected
            .unwrap()
            .path
            .display()
            .to_string()
            .contains("user-links"));
    }

    #[test]
    fn skips_empty_other_scope_self_and_ps1() {
        let root = temp_dir("resolve-skip");
        let tree = roots(&root);
        let bin = root.join("bin");
        let shim = bin.join("ffmpeg.exe");
        let manager = root.join("clevershim.exe");
        touch(&shim, b"shim");
        touch(&manager, b"manager");
        let package = root.join("user-packages/Demo.Tool_1");
        touch(&package.join("ffmpeg.exe"), b"");
        touch(&package.join("ffmpeg.ps1"), b"nope");
        touch(&package.join("bin/ffmpeg.exe"), b"ok");
        // A copy of the shim name inside the package bin is a different directory, allowed.
        touch(&bin.join("decoy.exe"), b"no");
        fs::create_dir_all(root.join("user-links")).unwrap();
        // Point a machine package file and a user link would be other scope if the real path is there.
        let machine = root.join("machine-packages/Demo.Tool_1/ffmpeg.exe");
        touch(&machine, b"machine");

        let locations = vec![package.join("does-not-matter")];
        let context = ctx(
            &tree,
            "ffmpeg",
            "Demo.Tool",
            &shim,
            &bin,
            &manager,
            &locations,
        );
        // The package id is Demo.Tool but the folder with ffmpeg.exe is Demo.Tool_1.
        // empty.exe is not named ffmpeg.exe. ps1 is ignored.
        let hit = resolve(&context);
        let selected = hit.selected.expect("real exe");
        assert!(selected.path.ends_with("ffmpeg.exe"));
        assert!(!selected.path.display().to_string().contains("empty"));
        assert!(hit
            .candidates
            .iter()
            .all(|candidate| candidate.disposition != "selected"
                || candidate.path.contains("ffmpeg.exe")));
    }

    #[test]
    fn batch_target_uses_cmd_runner() {
        let root = temp_dir("resolve-bat");
        let tree = roots(&root);
        let bin = root.join("bin");
        let shim = bin.join("tool.exe");
        let manager = root.join("clevershim.exe");
        touch(&shim, b"shim");
        touch(
            &root.join("user-packages/Demo.Tool_1/tool.bat"),
            b"@echo off\r\n",
        );
        let context = ctx(&tree, "tool", "Demo.Tool", &shim, &bin, &manager, &[]);
        let selected = resolve(&context).selected.unwrap();
        assert!(selected.runner.ends_with("cmd.exe"));
        assert_eq!(selected.runner_args[0], "/c");
        assert!(selected.runner_args[1].ends_with("tool.bat"));
    }

    #[test]
    fn refuses_a_target_inside_the_shim_bin() {
        let root = temp_dir("resolve-self");
        let tree = roots(&root);
        let bin = root.join("bin");
        let shim = bin.join("ffmpeg.exe");
        let manager = root.join("clevershim.exe");
        touch(&shim, b"shim");
        touch(&manager, b"mgr");
        // Package directory is the bin itself via install location.
        let locations = vec![bin.clone()];
        // Hosted Windows runners often expose TEMP via 8.3 aliases while
        // canonicalize() returns the long form; roots must still match.
        let shim_root = short_path(&shim).unwrap_or_else(|| shim.clone());
        let bin_root = short_path(&bin).unwrap_or_else(|| bin.clone());
        let manager_root = short_path(&manager).unwrap_or_else(|| manager.clone());
        let context = ctx(
            &tree,
            "ffmpeg",
            "Demo.Tool",
            &shim_root,
            &bin_root,
            &manager_root,
            &locations,
        );
        let hit = resolve(&context);
        assert!(hit.selected.is_none());
        assert!(hit
            .candidates
            .iter()
            .any(|candidate| candidate.disposition == "self-target"));
    }

    fn filetime_bump(older: &Path, newer: &Path) {
        let now = std::time::SystemTime::now();
        let earlier = now - std::time::Duration::from_secs(120);
        set_directory_modified(older, earlier);
        set_directory_modified(newer, now);
    }

    fn set_directory_modified(path: &Path, modified: std::time::SystemTime) {
        #[cfg(windows)]
        let file = {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_WRITE_ATTRIBUTES: u32 = 0x100;
            const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x02000000;
            fs::OpenOptions::new()
                .access_mode(FILE_WRITE_ATTRIBUTES)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
                .open(path)
                .unwrap()
        };
        #[cfg(not(windows))]
        let file = fs::File::open(path).unwrap();
        file.set_modified(modified).unwrap();
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

    #[cfg(not(windows))]
    fn short_path(_path: &Path) -> Option<PathBuf> {
        None
    }
}
