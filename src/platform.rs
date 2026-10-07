#[cfg(not(windows))]
use std::path::{Path, PathBuf};

#[cfg(not(windows))]
use crate::layout::Scope;

pub const TASK_NAME: &str = "Clevotec CleverShim Sync";
pub const UNINSTALL_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Clevotec.CleverShim";

#[cfg(windows)]
mod win {
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};

    use windows::core::{Interface, BSTR, PCWSTR, PWSTR};
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegEnumKeyExW, RegOpenKeyExW,
        RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ,
        KEY_SET_VALUE, KEY_WOW64_64KEY, REG_DWORD, REG_EXPAND_SZ, REG_SZ, REG_VALUE_TYPE,
    };
    use windows::Win32::System::TaskScheduler::{
        IExecAction, ILogonTrigger, ITaskService, TaskScheduler, TASK_ACTION_EXEC,
        TASK_CREATE_OR_UPDATE, TASK_LOGON_INTERACTIVE_TOKEN, TASK_RUNLEVEL_LUA, TASK_TRIGGER_LOGON,
    };
    use windows::Win32::System::Threading::CREATE_NO_WINDOW;
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Shell::IsUserAnAdmin;
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    use super::{TASK_NAME, UNINSTALL_KEY};
    use crate::layout::Scope;

    struct ComApartment;

    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }

    pub fn is_elevated() -> bool {
        unsafe { IsUserAnAdmin().as_bool() }
    }

    pub fn remove_self_after_exit(exe: &Path, system_root: &Path) -> Result<(), String> {
        // Windows pins a running image. Pass paths as data, not interpolated script text.
        Command::new(system_root.join("System32/WindowsPowerShell/v1.0/powershell.exe"))
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Wait-Process -Id $env:CLEVERSHIM_UNINSTALL_PID -ErrorAction SilentlyContinue; \
                 Remove-Item -LiteralPath $env:CLEVERSHIM_UNINSTALL_TARGET -Force -ErrorAction Stop",
            ])
            .env("CLEVERSHIM_UNINSTALL_PID", std::process::id().to_string())
            .env("CLEVERSHIM_UNINSTALL_TARGET", exe)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW.0)
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn read_path(scope: Scope) -> Result<String, String> {
        read_sz(root(scope), path_key(scope), "Path")
    }

    pub fn write_path(scope: Scope, value: &str) -> Result<(), String> {
        write_sz(root(scope), path_key(scope), "Path", value, REG_EXPAND_SZ)
    }

    pub fn broadcast_environment() -> Result<(), String> {
        let wide: Vec<u16> = "Environment"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM::default(),
                LPARAM(wide.as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                5000,
                None,
            );
        }
        Ok(())
    }

    pub fn install_logon_task(exe: &Path) -> Result<(), String> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(err)?;
            let _apartment = ComApartment;
            let service: ITaskService =
                CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER).map_err(err)?;
            service
                .Connect(
                    &VARIANT::default(),
                    &VARIANT::default(),
                    &VARIANT::default(),
                    &VARIANT::default(),
                )
                .map_err(err)?;
            let folder = service.GetFolder(&BSTR::from("\\")).map_err(err)?;
            let task = service.NewTask(0).map_err(err)?;
            let principal = task.Principal().map_err(err)?;
            principal
                .SetLogonType(TASK_LOGON_INTERACTIVE_TOKEN)
                .map_err(err)?;
            principal.SetRunLevel(TASK_RUNLEVEL_LUA).map_err(err)?;
            let triggers = task.Triggers().map_err(err)?;
            let trigger = triggers.Create(TASK_TRIGGER_LOGON).map_err(err)?;
            let logon: ILogonTrigger = trigger.cast().map_err(err)?;
            // Non-admin callers may register only their own user's logon trigger.
            let domain = service.ConnectedDomain().map_err(err)?;
            let user = service.ConnectedUser().map_err(err)?;
            let mut identity = Vec::with_capacity(domain.len() + user.len() + 1);
            identity.extend_from_slice(&domain);
            identity.push(b'\\' as u16);
            identity.extend_from_slice(&user);
            logon.SetUserId(&BSTR::from_wide(&identity)).map_err(err)?;
            let actions = task.Actions().map_err(err)?;
            let action = actions.Create(TASK_ACTION_EXEC).map_err(err)?;
            let exec: IExecAction = action.cast().map_err(err)?;
            exec.SetPath(&BSTR::from(exe.to_string_lossy().as_ref()))
                .map_err(err)?;
            exec.SetArguments(&BSTR::from("sync")).map_err(err)?;
            folder
                .RegisterTaskDefinition(
                    &BSTR::from(TASK_NAME),
                    &task,
                    TASK_CREATE_OR_UPDATE.0,
                    &VARIANT::default(),
                    &VARIANT::default(),
                    TASK_LOGON_INTERACTIVE_TOKEN,
                    &VARIANT::default(),
                )
                .map_err(err)?;
        }
        Ok(())
    }

    pub fn remove_logon_task() -> Result<(), String> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(err)?;
            let _apartment = ComApartment;
            let service: ITaskService =
                CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER).map_err(err)?;
            service
                .Connect(
                    &VARIANT::default(),
                    &VARIANT::default(),
                    &VARIANT::default(),
                    &VARIANT::default(),
                )
                .map_err(err)?;
            let folder = service.GetFolder(&BSTR::from("\\")).map_err(err)?;
            folder.DeleteTask(&BSTR::from(TASK_NAME), 0).map_err(err)?;
        }
        Ok(())
    }

    pub fn write_uninstall_key(
        scope: Scope,
        install_dir: &Path,
        exe: &Path,
        version: &str,
    ) -> Result<(), String> {
        let hive = match scope {
            Scope::User => HKEY_CURRENT_USER,
            Scope::Machine => HKEY_LOCAL_MACHINE,
        };
        let mut key = HKEY::default();
        let path = wide(UNINSTALL_KEY);
        unsafe {
            status(RegCreateKeyExW(
                hive,
                PCWSTR(path.as_ptr()),
                None,
                None,
                windows::Win32::System::Registry::REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE | KEY_WOW64_64KEY,
                None,
                &mut key,
                None,
            ))?;
            set_sz(key, "DisplayName", "CleverShim")?;
            set_sz(key, "DisplayVersion", version)?;
            set_sz(key, "Publisher", "Clevotec")?;
            set_sz(key, "InstallLocation", &install_dir.display().to_string())?;
            let uninstall = format!("\"{}\" /uninstall", exe.display());
            set_sz(key, "UninstallString", &uninstall)?;
            set_sz(key, "QuietUninstallString", &uninstall)?;
            set_dword(key, "NoModify", 1)?;
            set_dword(key, "NoRepair", 1)?;
            let _ = RegCloseKey(key);
        }
        Ok(())
    }

    pub fn remove_uninstall_key(scope: Scope) -> Result<(), String> {
        let hive = match scope {
            Scope::User => HKEY_CURRENT_USER,
            Scope::Machine => HKEY_LOCAL_MACHINE,
        };
        let path = wide(UNINSTALL_KEY);
        unsafe {
            status(RegDeleteTreeW(hive, PCWSTR(path.as_ptr())))?;
        }
        Ok(())
    }

    pub fn uninstall_locations(scope: Scope, package_id: &str) -> Vec<PathBuf> {
        UninstallIndex::scan(scope).matching(package_id)
    }

    /// Snapshot of Uninstall InstallLocation values so sync can avoid re-enumerating the registry
    /// once per catalog package.
    #[derive(Debug, Default, Clone)]
    pub struct UninstallIndex {
        entries: Vec<(String, PathBuf)>,
    }

    impl UninstallIndex {
        pub fn scan(scope: Scope) -> Self {
            let hive = match scope {
                Scope::User => HKEY_CURRENT_USER,
                Scope::Machine => HKEY_LOCAL_MACHINE,
            };
            let mut entries = Vec::new();
            collect_uninstall_entries(hive, &mut entries);
            Self { entries }
        }

        pub fn matching(&self, package_id: &str) -> Vec<PathBuf> {
            self.entries
                .iter()
                .filter(|(name, _)| crate::resolve::directory_matches_package(name, package_id))
                .map(|(_, path)| path.clone())
                .collect()
        }
    }

    fn collect_uninstall_entries(hive: HKEY, found: &mut Vec<(String, PathBuf)>) {
        let roots = [
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
            r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ];
        for root_path in roots {
            let mut root = HKEY::default();
            let wide_root = wide(root_path);
            let opened = unsafe {
                RegOpenKeyExW(
                    hive,
                    PCWSTR(wide_root.as_ptr()),
                    None,
                    KEY_READ | KEY_WOW64_64KEY,
                    &mut root,
                )
            };
            if opened.0 != 0 {
                continue;
            }
            let mut index = 0u32;
            loop {
                let mut name = [0u16; 512];
                let mut name_len = name.len() as u32;
                let status = unsafe {
                    RegEnumKeyExW(
                        root,
                        index,
                        Some(PWSTR(name.as_mut_ptr())),
                        &mut name_len,
                        None,
                        None,
                        None,
                        None,
                    )
                };
                if status.0 != 0 {
                    break;
                }
                let key_name = String::from_utf16_lossy(&name[..name_len as usize]);
                let sub = format!(r"{root_path}\{key_name}");
                if let Ok(location) = read_sz(hive, &sub, "InstallLocation") {
                    let location = location.trim().trim_end_matches('\\');
                    if !location.is_empty() {
                        found.push((key_name, PathBuf::from(location)));
                    }
                }
                index += 1;
            }
            unsafe {
                let _ = RegCloseKey(root);
            }
        }
    }

    fn root(scope: Scope) -> HKEY {
        match scope {
            Scope::User => HKEY_CURRENT_USER,
            Scope::Machine => HKEY_LOCAL_MACHINE,
        }
    }

    fn path_key(scope: Scope) -> &'static str {
        match scope {
            Scope::User => r"Environment",
            Scope::Machine => r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
        }
    }

    fn read_sz(hive: HKEY, subkey: &str, value: &str) -> Result<String, String> {
        let mut key = HKEY::default();
        let sub = wide(subkey);
        unsafe {
            status(RegOpenKeyExW(
                hive,
                PCWSTR(sub.as_ptr()),
                None,
                KEY_READ | KEY_WOW64_64KEY,
                &mut key,
            ))?;
            let name = wide(value);
            let mut kind = REG_VALUE_TYPE::default();
            let mut size = 0u32;
            let _ = RegQueryValueExW(
                key,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                None,
                Some(&mut size),
            );
            let mut buffer = vec![0u8; size as usize + 2];
            status(RegQueryValueExW(
                key,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                Some(buffer.as_mut_ptr()),
                Some(&mut size),
            ))?;
            let _ = RegCloseKey(key);
            let wide_buf: Vec<u16> = buffer[..size as usize]
                .chunks(2)
                .filter_map(|chunk| {
                    if chunk.len() == 2 {
                        Some(u16::from_le_bytes([chunk[0], chunk[1]]))
                    } else {
                        None
                    }
                })
                .take_while(|unit| *unit != 0)
                .collect();
            Ok(String::from_utf16_lossy(&wide_buf))
        }
    }

    fn write_sz(
        hive: HKEY,
        subkey: &str,
        value: &str,
        data: &str,
        kind: REG_VALUE_TYPE,
    ) -> Result<(), String> {
        let mut key = HKEY::default();
        let sub = wide(subkey);
        unsafe {
            status(RegCreateKeyExW(
                hive,
                PCWSTR(sub.as_ptr()),
                None,
                None,
                windows::Win32::System::Registry::REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE | KEY_WOW64_64KEY,
                None,
                &mut key,
                None,
            ))?;
            set_value(key, value, data, kind)?;
            let _ = RegCloseKey(key);
        }
        Ok(())
    }

    fn set_sz(key: HKEY, value: &str, data: &str) -> Result<(), String> {
        set_value(key, value, data, REG_SZ)
    }

    fn set_value(key: HKEY, value: &str, data: &str, kind: REG_VALUE_TYPE) -> Result<(), String> {
        let name = wide(value);
        let mut bytes: Vec<u8> = data
            .encode_utf16()
            .flat_map(|unit| unit.to_le_bytes())
            .collect();
        bytes.extend_from_slice(&[0, 0]);
        unsafe {
            status(RegSetValueExW(
                key,
                PCWSTR(name.as_ptr()),
                None,
                kind,
                Some(&bytes),
            ))?;
        }
        Ok(())
    }

    fn set_dword(key: HKEY, value: &str, data: u32) -> Result<(), String> {
        let name = wide(value);
        let bytes = data.to_le_bytes();
        unsafe {
            status(RegSetValueExW(
                key,
                PCWSTR(name.as_ptr()),
                None,
                REG_DWORD,
                Some(&bytes),
            ))?;
        }
        Ok(())
    }

    fn status(code: windows::Win32::Foundation::WIN32_ERROR) -> Result<(), String> {
        if code.0 == 0 {
            Ok(())
        } else {
            Err(format!("win32 error {}", code.0))
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn err(error: windows::core::Error) -> String {
        error.to_string()
    }
}

#[cfg(windows)]
pub use win::*;

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}

#[cfg(not(windows))]
pub fn read_path(_scope: Scope) -> Result<String, String> {
    Err("PATH registry edits run on Windows".into())
}

#[cfg(not(windows))]
pub fn write_path(_scope: Scope, _value: &str) -> Result<(), String> {
    Err("PATH registry edits run on Windows".into())
}

#[cfg(not(windows))]
pub fn broadcast_environment() -> Result<(), String> {
    Ok(())
}

#[cfg(not(windows))]
pub fn install_logon_task(_exe: &Path) -> Result<(), String> {
    Err("logon tasks run on Windows".into())
}

#[cfg(not(windows))]
pub fn remove_logon_task() -> Result<(), String> {
    Err("logon tasks run on Windows".into())
}

#[cfg(not(windows))]
pub fn write_uninstall_key(
    _scope: Scope,
    _install_dir: &Path,
    _exe: &Path,
    _version: &str,
) -> Result<(), String> {
    Err("uninstall keys run on Windows".into())
}

#[cfg(not(windows))]
pub fn remove_uninstall_key(_scope: Scope) -> Result<(), String> {
    Err("uninstall keys run on Windows".into())
}

#[cfg(not(windows))]
#[derive(Debug, Default, Clone)]
pub struct UninstallIndex;

#[cfg(not(windows))]
impl UninstallIndex {
    pub fn scan(_scope: Scope) -> Self {
        Self
    }

    pub fn matching(&self, _package_id: &str) -> Vec<PathBuf> {
        Vec::new()
    }
}

#[cfg(not(windows))]
pub fn uninstall_locations(_scope: Scope, _package_id: &str) -> Vec<PathBuf> {
    Vec::new()
}
