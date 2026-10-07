use std::fs;
use std::path::{Path, PathBuf};

use crate::catalog::CatalogFile;
use crate::layout::{Layout, Scope};
use crate::overrides::Overrides;
use crate::report::{CandidateNote, RepairReport};
use crate::resolve::{self, ResolvedTarget};
use crate::sidecar_format;
use crate::sync::{self, ProviderView, SyncRequest};
use crate::winget_list::WingetList;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Rewrite(ResolvedTarget),
    Retarget(ResolvedTarget),
    Unchanged,
    Remove,
}

pub fn decide(
    current_id: &str,
    current_target: &Path,
    winget: &WingetList,
    install_location_present: bool,
    package_dir_present: bool,
    current_resolved: Option<&ResolvedTarget>,
    alternative: Option<&ResolvedTarget>,
) -> Decision {
    let query_failed = matches!(winget, WingetList::Failed { .. });
    if query_failed {
        return rewrite_or_unchanged(current_target, current_resolved);
    }
    let gone = matches!(winget, WingetList::NotInstalled)
        && !install_location_present
        && !package_dir_present;
    if gone {
        if let Some(alternative) = alternative {
            if alternative.package_id.eq_ignore_ascii_case(current_id)
                && sidecar_format::paths_equal(current_target, &alternative.path)
            {
                return Decision::Unchanged;
            }
            return Decision::Retarget(alternative.clone());
        }
        return Decision::Remove;
    }
    rewrite_or_unchanged(current_target, current_resolved)
}

fn rewrite_or_unchanged(
    current_target: &Path,
    current_resolved: Option<&ResolvedTarget>,
) -> Decision {
    if let Some(resolved) = current_resolved {
        if !sidecar_format::paths_equal(current_target, &resolved.path) {
            return Decision::Rewrite(resolved.clone());
        }
    }
    Decision::Unchanged
}

pub struct RepairRequest<'a> {
    pub layout: &'a Layout,
    pub catalog: &'a CatalogFile,
    pub overrides: &'a Overrides,
    pub shim_exe: &'a Path,
    pub dry_run: bool,
    pub winget_for: &'a dyn Fn(&str) -> WingetList,
    pub install_locations: &'a dyn Fn(Scope, &str) -> Vec<PathBuf>,
    pub manager: &'a Path,
}

pub fn repair_shim(request: &RepairRequest<'_>) -> Result<RepairReport, String> {
    let sidecar_path = sidecar_format::sidecar_path_for_exe(request.shim_exe);
    let sidecar = sidecar_format::load(&sidecar_path).map_err(|err| err.to_string())?;
    let scope = Scope::parse(&sidecar.scope).unwrap_or(Scope::User);
    let command = if sidecar.command.is_empty() {
        request
            .shim_exe
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("shim")
            .to_string()
    } else {
        sidecar.command.clone()
    };
    let old_target = PathBuf::from(&sidecar.target);
    let old_exists = old_target.is_file();
    let winget = (request.winget_for)(&sidecar.package_id);
    let locations = (request.install_locations)(scope, &sidecar.package_id);
    let package_dirs = resolve::PackageDirIndex::scan(&request.layout.packages_root(scope));
    let package_dir_present = !package_dirs.matching(&sidecar.package_id).is_empty();
    let sync_request = SyncRequest {
        layout: request.layout,
        scope,
        catalog: request.catalog,
        overrides: request.overrides,
        stub: None,
        manager: request.manager,
        package_dirs: &package_dirs,
        install_locations: &|id: &str| (request.install_locations)(scope, id),
    };
    let current_package = request.catalog.get(&sidecar.package_id);
    let path_prefix = current_package
        .map(|package| package.archive_binaries_depend_on_path)
        .unwrap_or(false);
    let current_hit =
        sync::resolve_package(&sync_request, &sidecar.package_id, &command, path_prefix);
    let alternative = best_alternative(&sync_request, &command, &sidecar.package_id);
    let decision = decide(
        &sidecar.package_id,
        &old_target,
        &winget,
        !locations.is_empty(),
        package_dir_present,
        current_hit.selected.as_ref(),
        alternative.as_ref().and_then(|view| view.resolved.as_ref()),
    );

    let mut report = RepairReport {
        shim: command.clone(),
        scope: scope.as_str().into(),
        package_id: sidecar.package_id.clone(),
        old_target: sidecar.target.clone(),
        old_target_exists: old_exists,
        winget_list: winget.summary(),
        searched: current_hit.searched,
        candidates: current_hit.candidates,
        sidecar: "unchanged".into(),
        removed_shims: Vec::new(),
        next: "clevershim list".into(),
        success: false,
        removed: false,
    };
    if let Some(view) = &alternative {
        if let Some(resolved) = &view.resolved {
            report.candidates.push(CandidateNote {
                path: resolved.path.display().to_string(),
                disposition: format!("alternative {}", view.package_id),
            });
        }
    }

    match decision {
        Decision::Rewrite(resolved) | Decision::Retarget(resolved) => {
            let retarget = !resolved
                .package_id
                .eq_ignore_ascii_case(&sidecar.package_id);
            report.sidecar = if retarget {
                format!("retargeted to {}", resolved.package_id)
            } else {
                "rewritten".into()
            };
            report.success = resolved.path.is_file();
            report.next = "clevershim list".into();
            if !request.dry_run {
                let mut updated =
                    sync::sidecar_from_resolved(&resolved, &request.manager.display().to_string());
                updated.command = command;
                updated.scope = scope.as_str().into();
                updated.manager = request.manager.display().to_string();
                sidecar_format::store(&sidecar_path, &updated).map_err(|err| err.to_string())?;
            }
        }
        Decision::Remove => {
            report.sidecar = "removed".into();
            report.removed = true;
            report.removed_shims.push(command);
            report.success = false;
            report.next = "clevershim sync".into();
            if !request.dry_run {
                let _ = fs::remove_file(&sidecar_path);
                let _ = fs::remove_file(request.shim_exe);
            }
        }
        Decision::Unchanged => {
            report.sidecar = "unchanged".into();
            report.success = old_exists || current_hit.selected.is_some();
            report.next = if report.success {
                "clevershim list".into()
            } else {
                "clevershim sync".into()
            };
        }
    }
    Ok(report)
}

fn best_alternative(
    request: &SyncRequest<'_>,
    command: &str,
    current_id: &str,
) -> Option<ProviderView> {
    let mut views = Vec::new();
    for package in request.catalog.providers_for(command) {
        if package.id.eq_ignore_ascii_case(current_id) {
            continue;
        }
        let installed = sync::provider_installed(request, &package.id);
        let resolved = if installed {
            sync::resolve_package(
                request,
                &package.id,
                command,
                package.archive_binaries_depend_on_path,
            )
            .selected
        } else {
            None
        };
        views.push(ProviderView {
            package_id: package.id.clone(),
            installed,
            resolved,
            rank: request.overrides.rank(command, &package.id),
        });
    }
    sync::choose_provider(views)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::PackageEntry;
    use crate::sidecar_format::Sidecar;
    use crate::testutil::temp_dir;
    use crate::winget_list::WingetList;

    fn target(id: &str, path: &str) -> ResolvedTarget {
        ResolvedTarget {
            package_id: id.into(),
            path: PathBuf::from(path),
            working_directory: PathBuf::from("/opt"),
            runner: String::new(),
            runner_args: Vec::new(),
            path_prefix: false,
        }
    }

    #[test]
    fn one_decision_does_not_loop_and_refuses_nothing_here() {
        let current = Path::new(r"/old/ffmpeg.exe");
        let moved = target("Gyan.FFmpeg", "/new/ffmpeg.exe");
        assert_eq!(
            decide(
                "Gyan.FFmpeg",
                current,
                &WingetList::Installed {
                    version: "9".into()
                },
                false,
                true,
                Some(&moved),
                None
            ),
            Decision::Rewrite(moved)
        );
        assert_eq!(
            decide(
                "Gyan.FFmpeg",
                current,
                &WingetList::Failed {
                    message: "source offline".into()
                },
                false,
                false,
                None,
                Some(&target("yt-dlp.FFmpeg", "/other/ffmpeg.exe"))
            ),
            Decision::Unchanged
        );
    }

    #[test]
    fn gone_package_retargets_or_removes() {
        let alt = target("yt-dlp.FFmpeg", "/yt/ffmpeg.exe");
        assert_eq!(
            decide(
                "Gyan.FFmpeg",
                Path::new("/old/ffmpeg.exe"),
                &WingetList::NotInstalled,
                false,
                false,
                None,
                Some(&alt)
            ),
            Decision::Retarget(alt)
        );
        assert_eq!(
            decide(
                "Gyan.FFmpeg",
                Path::new("/old/ffmpeg.exe"),
                &WingetList::NotInstalled,
                false,
                false,
                None,
                None
            ),
            Decision::Remove
        );
        assert_eq!(
            decide(
                "Gyan.FFmpeg",
                Path::new("/old/ffmpeg.exe"),
                &WingetList::Installed {
                    version: "9".into()
                },
                false,
                false,
                None,
                None
            ),
            Decision::Unchanged
        );
    }

    #[test]
    fn repair_removes_shim_when_package_is_gone() {
        let root = temp_dir("repair-gone");
        let layout = Layout {
            local_app_data: root.join("local"),
            program_data: root.join("programdata"),
            program_files: root.join("programfiles"),
            system_root: root.join("Windows"),
        };
        let catalog = CatalogFile {
            packages: vec![PackageEntry {
                id: "Demo.Tool".into(),
                version: "1".into(),
                commands: vec!["demo".into()],
                archive_binaries_depend_on_path: false,
            }],
            collisions: Default::default(),
        };
        let overrides = Overrides::embedded().unwrap();
        let manager = layout.user_root().join("clevershim.exe");
        fs::create_dir_all(layout.user_bin()).unwrap();
        fs::write(&manager, b"mgr").unwrap();
        let shim = layout.user_bin().join("demo.exe");
        fs::write(&shim, b"shim").unwrap();
        sidecar_format::store(
            &layout.user_bin().join("demo.shim"),
            &Sidecar {
                command: "demo".into(),
                package_id: "Demo.Tool".into(),
                scope: "user".into(),
                target: root.join("missing.exe").display().to_string(),
                manager: manager.display().to_string(),
                ..Sidecar::default()
            },
        )
        .unwrap();
        let report = repair_shim(&RepairRequest {
            layout: &layout,
            catalog: &catalog,
            overrides: &overrides,
            shim_exe: &shim,
            dry_run: false,
            winget_for: &|_| WingetList::NotInstalled,
            install_locations: &|_scope, _id| Vec::new(),
            manager: &manager,
        })
        .unwrap();
        assert!(report.removed);
        assert!(report.render().contains("no longer installed"));
        assert!(!layout.user_bin().join("demo.shim").exists());
        assert_eq!(report.next, "clevershim sync");
    }

    #[test]
    fn repair_rewrites_sidecar_when_the_file_moved() {
        let root = temp_dir("repair-move");
        let layout = Layout {
            local_app_data: root.join("local"),
            program_data: root.join("programdata"),
            program_files: root.join("programfiles"),
            system_root: root.join("Windows"),
        };
        let catalog = CatalogFile {
            packages: vec![PackageEntry {
                id: "Demo.Tool".into(),
                version: "1".into(),
                commands: vec!["demo".into()],
                archive_binaries_depend_on_path: true,
            }],
            collisions: Default::default(),
        };
        let overrides = Overrides::default();
        let manager = layout.user_root().join("clevershim.exe");
        fs::create_dir_all(manager.parent().unwrap()).unwrap();
        fs::write(&manager, b"mgr").unwrap();
        let new_exe = layout.user_packages().join("Demo.Tool_2").join("demo.exe");
        fs::create_dir_all(new_exe.parent().unwrap()).unwrap();
        fs::write(&new_exe, b"new").unwrap();
        let shim = layout.user_bin().join("demo.exe");
        fs::create_dir_all(shim.parent().unwrap()).unwrap();
        fs::write(&shim, b"shim").unwrap();
        sidecar_format::store(
            &layout.user_bin().join("demo.shim"),
            &Sidecar {
                command: "demo".into(),
                package_id: "Demo.Tool".into(),
                scope: "user".into(),
                target: root.join("gone").join("demo.exe").display().to_string(),
                manager: manager.display().to_string(),
                ..Sidecar::default()
            },
        )
        .unwrap();
        let report = repair_shim(&RepairRequest {
            layout: &layout,
            catalog: &catalog,
            overrides: &overrides,
            shim_exe: &shim,
            dry_run: false,
            winget_for: &|_| WingetList::Installed {
                version: "2".into(),
            },
            install_locations: &|_scope, _id| Vec::new(),
            manager: &manager,
        })
        .unwrap();
        assert_eq!(report.sidecar, "rewritten");
        assert!(report.success);
        let updated = sidecar_format::load(&layout.user_bin().join("demo.shim")).unwrap();
        assert!(updated.target.contains("Demo.Tool_2"));
        assert!(updated.path_prefix);
        assert!(updated.runner.is_empty());
    }
}
