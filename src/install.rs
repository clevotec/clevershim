use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

use crate::catalog::{finalize_catalog, merge_packages, CatalogFile};
use crate::layout::{append_path_segment, remove_path_segment, Layout, Scope};
use crate::overrides::{self, Overrides};
use crate::platform;
use crate::sync::{self, SyncRequest};

pub fn setup() -> Result<()> {
    let layout = Layout::from_env();
    let elevated = platform::is_elevated();
    install_scope(&layout, Scope::User, true)?;
    if elevated {
        install_scope(&layout, Scope::Machine, true)?;
    }
    println!(
        "CleverShim installed for {}",
        if elevated {
            "this user and the machine"
        } else {
            "this user"
        }
    );
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let layout = Layout::from_env();
    remove_scope(&layout, Scope::User)?;
    if platform::is_elevated() {
        remove_scope(&layout, Scope::Machine)?;
    }
    println!("CleverShim removed");
    Ok(())
}

pub fn hook_install() -> Result<()> {
    let layout = Layout::from_env();
    let exe = installed_or_current(&layout, Scope::User)?;
    platform::install_logon_task(&exe).map_err(plain)?;
    println!("logon sync task installed");
    Ok(())
}

pub fn hook_remove() -> Result<()> {
    platform::remove_logon_task().map_err(plain)?;
    println!("logon sync task removed");
    Ok(())
}

pub fn sync_command() -> Result<()> {
    let layout = Layout::from_env();
    let stub = load_stub_bytes();
    run_sync(&layout, Scope::User, stub.as_deref())?;
    if machine_writable(&layout) {
        run_sync(&layout, Scope::Machine, stub.as_deref())?;
    }
    Ok(())
}

pub fn list_command() -> Result<()> {
    let layout = Layout::from_env();
    let mut any = false;
    for scope in [Scope::User, Scope::Machine] {
        let bin = layout.bin(scope);
        let entries = match fs::read_dir(&bin) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("shim") {
                continue;
            }
            any = true;
            match crate::sidecar_format::load(&path) {
                Ok(sidecar) => {
                    let exists = Path::new(&sidecar.target).is_file();
                    println!(
                        "{}  {}  {}  {}  {}",
                        scope.as_str(),
                        sidecar.command,
                        sidecar.package_id,
                        sidecar.target,
                        if exists { "ok" } else { "missing" }
                    );
                }
                Err(err) => println!("{}  {}  unreadable: {err}", scope.as_str(), path.display()),
            }
        }
    }
    if !any {
        println!("no shims yet; run clevershim sync");
    }
    Ok(())
}

pub fn repair_command(
    name: Option<&str>,
    shim: Option<&Path>,
    explain: bool,
    dry_run: bool,
    from_shim: bool,
) -> Result<i32> {
    let layout = Layout::from_env();
    let targets = shim_targets(&layout, name, shim)?;
    if targets.is_empty() {
        let message = "no matching shim";
        if from_shim {
            eprintln!("clevershim: {message}");
        } else {
            println!("{message}");
        }
        return Ok(1);
    }
    let mut exit = 0;
    for shim_exe in targets {
        let scope = scope_of_shim(&layout, &shim_exe);
        let catalog = catalog_for(&layout, scope)?;
        let overrides = overrides_for(&layout, scope)?;
        let manager = manager_for(&layout, scope);
        let report = crate::repair::repair_shim(&crate::repair::RepairRequest {
            layout: &layout,
            catalog: &catalog,
            overrides: &overrides,
            shim_exe: &shim_exe,
            dry_run,
            winget_for: &crate::winget_list::query_winget,
            install_locations: &|scope, id| platform::uninstall_locations(scope, id),
            manager: &manager,
        })
        .map_err(plain)?;
        if from_shim {
            if report.removed {
                eprintln!("{}", report.render());
                exit = 3;
            } else if !report.success {
                eprintln!("{}", report.render());
                exit = 1;
            }
        } else if explain || !report.success || report.removed {
            let stream = if report.success && !report.removed {
                println!("{}", report.render());
                true
            } else {
                eprintln!("{}", report.render());
                false
            };
            let _ = stream;
            if report.removed {
                exit = 3;
            } else if !report.success {
                exit = 1;
            }
        } else {
            println!("{} {}", report.shim, report.sidecar);
        }
    }
    Ok(exit)
}

fn install_scope(layout: &Layout, scope: Scope, with_task: bool) -> Result<()> {
    let root = layout.root(scope);
    fs::create_dir_all(layout.bin(scope))?;
    let exe = root.join(if cfg!(windows) {
        "clevershim.exe"
    } else {
        "clevershim"
    });
    copy_self(&exe)?;
    if let Some(bytes) = load_stub_bytes() {
        let stub_name = if cfg!(windows) {
            "clevershim-shim.exe"
        } else {
            "clevershim-shim"
        };
        fs::write(root.join(stub_name), bytes)?;
    }
    let path_value = platform::read_path(scope).unwrap_or_default();
    let updated = append_path_segment(&path_value, &layout.bin(scope).display().to_string());
    platform::write_path(scope, &updated).map_err(plain)?;
    let _ = platform::broadcast_environment();
    run_sync(layout, scope, load_stub_bytes().as_deref())?;
    if with_task && scope == Scope::User {
        platform::install_logon_task(&exe).map_err(plain)?;
    }
    platform::write_uninstall_key(scope, &root, &exe, env!("CARGO_PKG_VERSION")).map_err(plain)?;
    Ok(())
}

fn remove_scope(layout: &Layout, scope: Scope) -> Result<()> {
    if scope == Scope::User {
        let _ = platform::remove_logon_task();
    }
    if let Ok(path_value) = platform::read_path(scope) {
        let updated = remove_path_segment(&path_value, &layout.bin(scope).display().to_string());
        let _ = platform::write_path(scope, &updated);
        let _ = platform::broadcast_environment();
    }
    if layout.bin(scope).exists() {
        let _ = fs::remove_dir_all(layout.bin(scope));
    }
    let _ = platform::remove_uninstall_key(scope);
    let root = layout.root(scope);
    if root.exists() {
        for entry in fs::read_dir(&root).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_file() && fs::remove_file(&path).is_err() {
                #[cfg(windows)]
                if env::current_exe()
                    .map(|exe| crate::sidecar_format::paths_equal(&path, &exe))
                    .unwrap_or(false)
                {
                    platform::remove_self_after_exit(&path, &layout.system_root).map_err(plain)?;
                }
            }
        }
    }
    Ok(())
}

fn run_sync(layout: &Layout, scope: Scope, stub: Option<&[u8]>) -> Result<()> {
    let catalog = catalog_for(layout, scope)?;
    let overrides = overrides_for(layout, scope)?;
    let manager = manager_for(layout, scope);
    let notes = sync::sync_scope(&SyncRequest {
        layout,
        scope,
        catalog: &catalog,
        overrides: &overrides,
        stub,
        manager: &manager,
        install_locations: &|id| platform::uninstall_locations(scope, id),
    })
    .map_err(plain)?;
    for note in notes {
        println!("{note}");
    }
    Ok(())
}

pub fn catalog_for(layout: &Layout, scope: Scope) -> Result<CatalogFile> {
    let base = CatalogFile::embedded().map_err(|err| anyhow!(err))?;
    let overrides = overrides_for(layout, scope)?;
    let packages = if scope == Scope::User && layout.user_extra_catalog().is_file() {
        let extra = CatalogFile::load(&layout.user_extra_catalog()).map_err(|err| anyhow!(err))?;
        merge_packages(&base, &extra)
    } else {
        base.packages.clone()
    };
    Ok(finalize_catalog(packages, &overrides))
}

pub fn overrides_for(layout: &Layout, scope: Scope) -> Result<Overrides> {
    let user = if scope == Scope::User {
        Some(layout.user_overrides())
    } else {
        None
    };
    overrides::load_layered(Some(&layout.machine_overrides()), user.as_deref())
        .map_err(|err| anyhow!(err))
}

fn manager_for(layout: &Layout, scope: Scope) -> PathBuf {
    let installed = layout.manager_path(scope);
    if installed.is_file() {
        installed
    } else {
        env::current_exe().unwrap_or(installed)
    }
}

fn installed_or_current(layout: &Layout, scope: Scope) -> Result<PathBuf> {
    Ok(manager_for(layout, scope))
}

fn copy_self(dest: &Path) -> Result<()> {
    let current = env::current_exe().context("finding clevershim.exe")?;
    if sidecar_paths_equal(&current, dest) {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&current, dest)
        .with_context(|| format!("copying clevershim to {}", dest.display()))?;
    Ok(())
}

fn sidecar_paths_equal(left: &Path, right: &Path) -> bool {
    crate::sidecar_format::paths_equal(left, right)
}

fn machine_writable(layout: &Layout) -> bool {
    fs::create_dir_all(layout.machine_bin()).is_ok()
        && fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(layout.machine_bin().join(".lock"))
            .is_ok()
}

fn scope_of_shim(layout: &Layout, shim: &Path) -> Scope {
    if crate::sidecar_format::path_is_inside(shim, &layout.machine_bin()) {
        Scope::Machine
    } else {
        Scope::User
    }
}

fn shim_targets(layout: &Layout, name: Option<&str>, shim: Option<&Path>) -> Result<Vec<PathBuf>> {
    if let Some(path) = shim {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut found = Vec::new();
    for scope in [Scope::User, Scope::Machine] {
        let bin = layout.bin(scope);
        let entries = match fs::read_dir(&bin) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("shim") {
                continue;
            }
            let mut exe = path.clone();
            exe.set_extension("exe");
            if let Some(name) = name {
                let stem = exe.file_stem().and_then(|stem| stem.to_str()).unwrap_or("");
                let sidecar = crate::sidecar_format::load(&path).ok();
                let command = sidecar
                    .as_ref()
                    .map(|item| item.command.as_str())
                    .unwrap_or(stem);
                if !command.eq_ignore_ascii_case(name) && !stem.eq_ignore_ascii_case(name) {
                    continue;
                }
            }
            found.push(exe);
        }
    }
    Ok(found)
}

fn plain(err: String) -> anyhow::Error {
    anyhow!(err)
}

pub fn load_stub_bytes() -> Option<Vec<u8>> {
    if let Some(path) = env::var_os("CLEVERSHIM_SHIM_STUB") {
        if let Ok(bytes) = fs::read(path) {
            if !bytes.is_empty() {
                return Some(bytes);
            }
        }
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in ["clevershim-shim.exe", "clevershim-shim"] {
                let path = dir.join(name);
                if let Ok(bytes) = fs::read(&path) {
                    if !bytes.is_empty() && path != exe {
                        return Some(bytes);
                    }
                }
            }
        }
    }
    #[cfg(embedded_shim)]
    {
        return Some(include_bytes!("../assets/clevershim-shim.exe").to_vec());
    }
    #[cfg(not(embedded_shim))]
    None
}
