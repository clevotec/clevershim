use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use fs2::FileExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    User,
    Machine,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::User => "user",
            Scope::Machine => "machine",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "user" => Some(Scope::User),
            "machine" => Some(Scope::Machine),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Layout {
    pub local_app_data: PathBuf,
    pub program_data: PathBuf,
    pub program_files: PathBuf,
    pub system_root: PathBuf,
}

impl Layout {
    pub fn from_env() -> Self {
        Self {
            local_app_data: env_path(
                "CLEVERSHIM_LOCALAPPDATA",
                "LOCALAPPDATA",
                home_join("AppData/Local"),
            ),
            program_data: env_path(
                "CLEVERSHIM_PROGRAMDATA",
                "ProgramData",
                PathBuf::from("/ProgramData"),
            ),
            program_files: env_path(
                "CLEVERSHIM_PROGRAMFILES",
                "ProgramFiles",
                PathBuf::from("/Program Files"),
            ),
            system_root: env_path(
                "CLEVERSHIM_SYSTEMROOT",
                "SystemRoot",
                PathBuf::from("/Windows"),
            ),
        }
    }

    pub fn user_root(&self) -> PathBuf {
        self.local_app_data.join("Clevotec").join("CleverShim")
    }

    pub fn user_bin(&self) -> PathBuf {
        self.user_root().join("bin")
    }

    pub fn machine_root(&self) -> PathBuf {
        self.program_data.join("Clevotec").join("CleverShim")
    }

    pub fn machine_bin(&self) -> PathBuf {
        self.machine_root().join("bin")
    }

    pub fn bin(&self, scope: Scope) -> PathBuf {
        match scope {
            Scope::User => self.user_bin(),
            Scope::Machine => self.machine_bin(),
        }
    }

    pub fn root(&self, scope: Scope) -> PathBuf {
        match scope {
            Scope::User => self.user_root(),
            Scope::Machine => self.machine_root(),
        }
    }

    pub fn manager_path(&self, scope: Scope) -> PathBuf {
        let name = if cfg!(windows) {
            "clevershim.exe"
        } else {
            "clevershim"
        };
        self.root(scope).join(name)
    }

    pub fn user_packages(&self) -> PathBuf {
        self.local_app_data
            .join("Microsoft")
            .join("WinGet")
            .join("Packages")
    }

    pub fn machine_packages(&self) -> PathBuf {
        self.program_files.join("WinGet").join("Packages")
    }

    pub fn packages_root(&self, scope: Scope) -> PathBuf {
        match scope {
            Scope::User => self.user_packages(),
            Scope::Machine => self.machine_packages(),
        }
    }

    pub fn user_links(&self) -> PathBuf {
        self.local_app_data
            .join("Microsoft")
            .join("WinGet")
            .join("Links")
    }

    pub fn machine_links(&self) -> PathBuf {
        self.program_files.join("WinGet").join("Links")
    }

    pub fn links_root(&self, scope: Scope) -> PathBuf {
        match scope {
            Scope::User => self.user_links(),
            Scope::Machine => self.machine_links(),
        }
    }

    pub fn user_extra_catalog(&self) -> PathBuf {
        self.local_app_data.join("clevershim").join("packages.yaml")
    }

    pub fn user_overrides(&self) -> PathBuf {
        self.user_root().join("overrides.yaml")
    }

    pub fn machine_overrides(&self) -> PathBuf {
        self.machine_root().join("overrides.yaml")
    }

    pub fn cmd_exe(&self) -> PathBuf {
        self.system_root.join("System32").join("cmd.exe")
    }
}

fn env_path(primary: &str, secondary: &str, fallback: PathBuf) -> PathBuf {
    env::var_os(primary)
        .or_else(|| env::var_os(secondary))
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(fallback)
}

fn home_join(suffix: &str) -> PathBuf {
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(suffix)
}

pub struct BinLock {
    _file: fs::File,
}

impl BinLock {
    pub fn acquire(bin: &Path) -> std::io::Result<Self> {
        fs::create_dir_all(bin)?;
        let file = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(bin.join(".lock"))?;
        file.lock_exclusive()?;
        Ok(Self { _file: file })
    }
}

/// Append `segment` at the end of a PATH string when it is not already present.
pub fn append_path_segment(existing: &str, segment: &str) -> String {
    if path_contains(existing, segment) {
        return existing.trim().trim_matches(';').to_string();
    }
    let existing = existing.trim().trim_matches(';');
    if existing.is_empty() {
        segment.to_string()
    } else {
        format!("{existing};{segment}")
    }
}

pub fn remove_path_segment(existing: &str, segment: &str) -> String {
    existing
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty() && !path_segments_equal(part, segment))
        .collect::<Vec<_>>()
        .join(";")
}

pub fn path_contains(existing: &str, segment: &str) -> bool {
    existing
        .split(';')
        .map(str::trim)
        .any(|part| path_segments_equal(part, segment))
}

fn path_segments_equal(left: &str, right: &str) -> bool {
    normalize_segment(left) == normalize_segment(right)
}

fn normalize_segment(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_end_matches(['\\', '/'])
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_append_is_last_and_does_not_duplicate() {
        let once = append_path_segment(r"C:\Windows;C:\Tools", r"C:\Users\A\bin");
        assert_eq!(once, r"C:\Windows;C:\Tools;C:\Users\A\bin");
        let twice = append_path_segment(&once, r"C:\Users\A\bin\");
        assert_eq!(twice, once);
        assert!(path_contains(&twice, r"c:\users\a\bin"));
    }

    #[test]
    fn path_remove_drops_only_our_segment() {
        let value = r"C:\Windows;C:\Users\A\bin;C:\Tools";
        assert_eq!(
            remove_path_segment(value, r"C:\Users\A\bin"),
            r"C:\Windows;C:\Tools"
        );
    }
}
