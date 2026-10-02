use std::fs;
use std::path::{Path, PathBuf};

use crate::catalog::CatalogFile;
use crate::layout::{BinLock, Layout, Scope};
use crate::overrides::Overrides;
use crate::resolve::{self, ResolveContext, ResolvedTarget};
use crate::sidecar_format::{self, Sidecar};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncAction {
    Write(Sidecar),
    Remove,
    Leave,
}

#[derive(Debug, Clone)]
pub struct ProviderView {
    pub package_id: String,
    pub installed: bool,
    pub resolved: Option<ResolvedTarget>,
    pub rank: (u32, String),
}

pub fn choose_provider(mut providers: Vec<ProviderView>) -> Option<ProviderView> {
    providers.retain(|provider| provider.installed && provider.resolved.is_some());
    providers.sort_by(|left, right| left.rank.cmp(&right.rank));
    providers.into_iter().next()
}

pub fn action_for_command(
    any_installed: bool,
    chosen: Option<&ProviderView>,
    had_shim: bool,
) -> SyncAction {
    if let Some(provider) = chosen {
        let resolved = provider
            .resolved
            .as_ref()
            .expect("chosen providers have a resolved file");
        return SyncAction::Write(sidecar_from_resolved(resolved, ""));
    }
    if any_installed || !had_shim {
        SyncAction::Leave
    } else {
        SyncAction::Remove
    }
}

pub fn sidecar_from_resolved(resolved: &ResolvedTarget, manager: &str) -> Sidecar {
    Sidecar {
        command: String::new(),
        package_id: resolved.package_id.clone(),
        scope: String::new(),
        target: resolved.path.display().to_string(),
        runner: resolved.runner.clone(),
        runner_args: resolved.runner_args.clone(),
        working_directory: resolved.working_directory.display().to_string(),
        path_prefix: resolved.path_prefix,
        manager: manager.to_string(),
    }
}

pub struct SyncRequest<'a> {
    pub layout: &'a Layout,
    pub scope: Scope,
    pub catalog: &'a CatalogFile,
    pub overrides: &'a Overrides,
    pub stub: Option<&'a [u8]>,
    pub manager: &'a Path,
    pub install_locations: &'a dyn Fn(&str) -> Vec<PathBuf>,
}

pub fn sync_scope(request: &SyncRequest<'_>) -> Result<Vec<String>, String> {
    let _lock =
        BinLock::acquire(&request.layout.bin(request.scope)).map_err(|err| err.to_string())?;
    let mut notes = Vec::new();
    let commands = commands_in(request.catalog);
    let bin = request.layout.bin(request.scope);
    fs::create_dir_all(&bin).map_err(|err| err.to_string())?;

    let mut keep = Vec::new();
    for command in &commands {
        let views = provider_views(request, command);
        let any_installed = views.iter().any(|provider| provider.installed);
        let chosen = choose_provider(views);
        let shim_exe = bin.join(resolve::shim_file_name(command));
        let sidecar_path = sidecar_format::sidecar_path_for_exe(&shim_exe);
        let had_shim = sidecar_path.is_file();
        match action_for_command(any_installed, chosen.as_ref(), had_shim) {
            SyncAction::Write(mut sidecar) => {
                if let Some(provider) = &chosen {
                    if let Some(resolved) = &provider.resolved {
                        sidecar =
                            sidecar_from_resolved(resolved, &request.manager.display().to_string());
                    }
                }
                sidecar.command = command.clone();
                sidecar.scope = request.scope.as_str().to_string();
                sidecar.manager = request.manager.display().to_string();
                sidecar_format::store(&sidecar_path, &sidecar).map_err(|err| err.to_string())?;
                if let Some(bytes) = request.stub {
                    let _ = fs::write(&shim_exe, bytes);
                } else if !shim_exe.is_file() {
                    let _ = fs::write(&shim_exe, b"");
                }
                notes.push(format!(
                    "write {command} -> {} ({})",
                    sidecar.target, sidecar.package_id
                ));
                keep.push(shim_exe);
            }
            SyncAction::Remove => {
                delete_shim(&shim_exe);
                notes.push(format!("remove {command}"));
            }
            SyncAction::Leave => {
                if had_shim {
                    keep.push(shim_exe);
                    notes.push(format!("keep {command}"));
                }
            }
        }
    }
    remove_orphans(&bin, &keep)?;
    Ok(notes)
}

fn commands_in(catalog: &CatalogFile) -> Vec<String> {
    let mut commands = Vec::new();
    for package in &catalog.packages {
        for command in &package.commands {
            if !commands
                .iter()
                .any(|have: &String| have.eq_ignore_ascii_case(command))
            {
                commands.push(command.clone());
            }
        }
    }
    commands.sort();
    commands
}

fn provider_views(request: &SyncRequest<'_>, command: &str) -> Vec<ProviderView> {
    request
        .catalog
        .providers_for(command)
        .into_iter()
        .map(|package| {
            let installed = provider_installed(request, &package.id);
            let resolved = if installed {
                resolve_package(
                    request,
                    &package.id,
                    command,
                    package.archive_binaries_depend_on_path,
                )
                .selected
            } else {
                None
            };
            ProviderView {
                package_id: package.id.clone(),
                installed,
                resolved,
                rank: request.overrides.rank(command, &package.id),
            }
        })
        .collect()
}

pub fn provider_installed(request: &SyncRequest<'_>, package_id: &str) -> bool {
    !resolve::matching_package_dirs(&request.layout.packages_root(request.scope), package_id)
        .is_empty()
        || !(request.install_locations)(package_id).is_empty()
}

pub fn resolve_package(
    request: &SyncRequest<'_>,
    package_id: &str,
    command: &str,
    path_prefix: bool,
) -> resolve::ResolveHit {
    let other = match request.scope {
        Scope::User => Scope::Machine,
        Scope::Machine => Scope::User,
    };
    let shim_exe = request
        .layout
        .bin(request.scope)
        .join(resolve::shim_file_name(command));
    let locations = (request.install_locations)(package_id);
    let ctx = ResolveContext {
        scope: request.scope,
        package_id,
        command,
        packages_root: &request.layout.packages_root(request.scope),
        links_root: &request.layout.links_root(request.scope),
        other_packages_root: &request.layout.packages_root(other),
        other_links_root: &request.layout.links_root(other),
        install_locations: &locations,
        shim_exe: &shim_exe,
        bin_dir: &request.layout.bin(request.scope),
        manager: request.manager,
        system_root: &request.layout.system_root,
        path_prefix,
    };
    resolve::resolve(&ctx)
}

fn delete_shim(exe: &Path) {
    let sidecar = sidecar_format::sidecar_path_for_exe(exe);
    let _ = fs::remove_file(sidecar);
    let _ = fs::remove_file(exe);
}

fn remove_orphans(bin: &Path, keep: &[PathBuf]) -> Result<(), String> {
    let entries = match fs::read_dir(bin) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == ".lock" || !name.ends_with(".shim") {
            continue;
        }
        let mut exe = path.clone();
        exe.set_extension("exe");
        if keep
            .iter()
            .any(|kept| sidecar_format::paths_equal(kept, &exe))
        {
            continue;
        }
        delete_shim(&exe);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{finalize_catalog, PackageEntry};
    use crate::overrides::Overrides;
    use crate::testutil::temp_dir;
    use std::fs;

    fn layout_at(root: &Path) -> Layout {
        Layout {
            local_app_data: root.join("local"),
            program_data: root.join("programdata"),
            program_files: root.join("programfiles"),
            system_root: root.join("Windows"),
        }
    }

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, b"exe").unwrap();
    }

    fn catalog() -> CatalogFile {
        finalize_catalog(
            vec![
                PackageEntry {
                    id: "yt-dlp.FFmpeg".into(),
                    version: "1".into(),
                    commands: vec!["ffmpeg".into(), "ffprobe".into(), "ffplay".into()],
                    archive_binaries_depend_on_path: true,
                },
                PackageEntry {
                    id: "Gyan.FFmpeg".into(),
                    version: "9".into(),
                    commands: vec!["ffmpeg".into(), "ffprobe".into(), "ffplay".into()],
                    archive_binaries_depend_on_path: false,
                },
                PackageEntry {
                    id: "Other.FFmpeg".into(),
                    version: "1".into(),
                    commands: vec!["ffmpeg".into()],
                    archive_binaries_depend_on_path: false,
                },
            ],
            &Overrides::embedded().unwrap(),
        )
    }

    #[test]
    fn priority_picks_yt_dlp_then_gyan_then_other() {
        let overrides = Overrides::embedded().unwrap();
        let views = vec![
            ProviderView {
                package_id: "Other.FFmpeg".into(),
                installed: true,
                resolved: Some(fake("Other.FFmpeg")),
                rank: overrides.rank("ffmpeg", "Other.FFmpeg"),
            },
            ProviderView {
                package_id: "Gyan.FFmpeg".into(),
                installed: true,
                resolved: Some(fake("Gyan.FFmpeg")),
                rank: overrides.rank("ffmpeg", "Gyan.FFmpeg"),
            },
            ProviderView {
                package_id: "yt-dlp.FFmpeg".into(),
                installed: true,
                resolved: Some(fake("yt-dlp.FFmpeg")),
                rank: overrides.rank("ffmpeg", "yt-dlp.FFmpeg"),
            },
        ];
        assert_eq!(choose_provider(views).unwrap().package_id, "yt-dlp.FFmpeg");
        let without_ytdlp = vec![
            ProviderView {
                package_id: "Other.FFmpeg".into(),
                installed: true,
                resolved: Some(fake("Other.FFmpeg")),
                rank: overrides.rank("ffmpeg", "Other.FFmpeg"),
            },
            ProviderView {
                package_id: "Gyan.FFmpeg".into(),
                installed: false,
                resolved: None,
                rank: overrides.rank("ffmpeg", "Gyan.FFmpeg"),
            },
        ];
        assert_eq!(
            choose_provider(without_ytdlp).unwrap().package_id,
            "Other.FFmpeg"
        );
    }

    #[test]
    fn absent_package_drops_only_that_scope() {
        let root = temp_dir("sync-scope");
        let layout = layout_at(&root);
        let catalog = catalog();
        let overrides = Overrides::embedded().unwrap();
        touch(
            &layout
                .user_packages()
                .join("Gyan.FFmpeg_1")
                .join("ffmpeg.exe"),
        );
        touch(
            &layout
                .machine_packages()
                .join("Gyan.FFmpeg_1")
                .join("ffmpeg.exe"),
        );
        let none = |_id: &str| Vec::new();
        let manager = layout.user_root().join("clevershim.exe");
        touch(&manager);
        sync_scope(&SyncRequest {
            layout: &layout,
            scope: Scope::User,
            catalog: &catalog,
            overrides: &overrides,
            stub: Some(b"stub"),
            manager: &manager,
            install_locations: &none,
        })
        .unwrap();
        let machine_manager = layout.machine_root().join("clevershim.exe");
        touch(&machine_manager);
        sync_scope(&SyncRequest {
            layout: &layout,
            scope: Scope::Machine,
            catalog: &catalog,
            overrides: &overrides,
            stub: Some(b"stub"),
            manager: &machine_manager,
            install_locations: &none,
        })
        .unwrap();
        assert!(layout.user_bin().join("ffmpeg.shim").is_file());
        assert!(layout.machine_bin().join("ffmpeg.shim").is_file());

        fs::remove_dir_all(layout.user_packages()).unwrap();
        let notes = sync_scope(&SyncRequest {
            layout: &layout,
            scope: Scope::User,
            catalog: &catalog,
            overrides: &overrides,
            stub: Some(b"stub"),
            manager: &manager,
            install_locations: &none,
        })
        .unwrap();
        assert!(notes.iter().any(|note| note.contains("remove ffmpeg")));
        assert!(!layout.user_bin().join("ffmpeg.shim").exists());
        assert!(layout.machine_bin().join("ffmpeg.shim").is_file());
    }

    #[test]
    fn installed_package_without_exe_is_not_removed() {
        let root = temp_dir("sync-keep");
        let layout = layout_at(&root);
        let catalog = catalog();
        let overrides = Overrides::embedded().unwrap();
        let package = layout.user_packages().join("Gyan.FFmpeg_1");
        fs::create_dir_all(&package).unwrap();
        let manager = layout.user_root().join("clevershim.exe");
        touch(&manager);
        let shim = layout.user_bin().join("ffmpeg.exe");
        touch(&shim);
        sidecar_format::store(
            &layout.user_bin().join("ffmpeg.shim"),
            &Sidecar {
                command: "ffmpeg".into(),
                package_id: "Gyan.FFmpeg".into(),
                scope: "user".into(),
                target: package.join("missing.exe").display().to_string(),
                manager: manager.display().to_string(),
                ..Sidecar::default()
            },
        )
        .unwrap();
        let none = |_id: &str| Vec::new();
        let notes = sync_scope(&SyncRequest {
            layout: &layout,
            scope: Scope::User,
            catalog: &catalog,
            overrides: &overrides,
            stub: Some(b"stub"),
            manager: &manager,
            install_locations: &none,
        })
        .unwrap();
        assert!(notes.iter().any(|note| note.contains("keep ffmpeg")));
        assert!(layout.user_bin().join("ffmpeg.shim").is_file());
    }

    fn fake(id: &str) -> ResolvedTarget {
        ResolvedTarget {
            package_id: id.into(),
            path: PathBuf::from(format!("/opt/{id}/ffmpeg.exe")),
            working_directory: PathBuf::from(format!("/opt/{id}")),
            runner: String::new(),
            runner_args: Vec::new(),
            path_prefix: false,
        }
    }
}
