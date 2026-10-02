use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForceInclude {
    pub id: String,
    pub commands: Vec<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub archive_binaries_depend_on_path: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overrides {
    pub priority: BTreeMap<String, Vec<String>>,
    pub force_include: Vec<ForceInclude>,
    pub force_exclude: BTreeSet<String>,
}

#[derive(Debug, Deserialize)]
struct OverridesFile {
    #[serde(default)]
    priority: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    force_include: Vec<ForceInclude>,
    #[serde(default)]
    force_exclude: Vec<String>,
}

impl Overrides {
    pub fn parse(text: &str) -> Result<Self, String> {
        let file: OverridesFile = serde_yaml::from_str(text).map_err(|err| err.to_string())?;
        Ok(Self {
            priority: file.priority,
            force_include: file.force_include,
            force_exclude: file.force_exclude.into_iter().collect(),
        })
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path).map_err(|err| err.to_string())?;
        Self::parse(&text)
    }

    pub fn embedded() -> Result<Self, String> {
        Self::parse(crate::catalog::EMBEDDED_OVERRIDES)
    }

    pub fn is_force_excluded(&self, id: &str) -> bool {
        self.force_exclude
            .iter()
            .any(|item| item.eq_ignore_ascii_case(id))
    }

    /// Lower rank is a better shim target. Unknown providers sort after the list, by id.
    pub fn rank(&self, command: &str, package_id: &str) -> (u32, String) {
        let fallback = package_id.to_ascii_lowercase();
        let Some(list) = self
            .priority
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(command))
            .map(|(_, ids)| ids)
        else {
            return (10_000, fallback);
        };
        if let Some(index) = list
            .iter()
            .position(|id| id.eq_ignore_ascii_case(package_id))
        {
            (index as u32, fallback)
        } else {
            (10_000, fallback)
        }
    }
}

/// Machine overrides sit on the embedded file. A user file applies only for that user.
pub fn merge_overrides(layers: &[Overrides]) -> Overrides {
    let mut merged = Overrides::default();
    for layer in layers {
        for (command, ids) in &layer.priority {
            merged.priority.insert(command.clone(), ids.clone());
        }
        for include in &layer.force_include {
            if let Some(existing) = merged
                .force_include
                .iter_mut()
                .find(|item| item.id.eq_ignore_ascii_case(&include.id))
            {
                *existing = include.clone();
            } else {
                merged.force_include.push(include.clone());
            }
        }
        for id in &layer.force_exclude {
            merged.force_exclude.insert(id.clone());
        }
    }
    merged
}

pub fn load_layered(machine: Option<&Path>, user: Option<&Path>) -> Result<Overrides, String> {
    let mut layers = vec![Overrides::embedded()?];
    if let Some(path) = machine {
        if path.is_file() {
            layers.push(Overrides::load(path)?);
        }
    }
    if let Some(path) = user {
        if path.is_file() {
            layers.push(Overrides::load(path)?);
        }
    }
    Ok(merge_overrides(&layers))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffmpeg_priority_prefers_yt_dlp_then_gyan() {
        let overrides = Overrides::embedded().unwrap();
        assert!(
            overrides.rank("ffmpeg", "yt-dlp.FFmpeg") < overrides.rank("ffmpeg", "Gyan.FFmpeg")
        );
        assert!(overrides.rank("ffmpeg", "Gyan.FFmpeg") < overrides.rank("ffmpeg", "Other.FFmpeg"));
        assert!(overrides.is_force_excluded("Clevotec.CleverShim"));
    }

    #[test]
    fn user_overrides_do_not_leak_into_another_user() {
        let dir = crate::testutil::temp_dir("overrides");
        let machine = dir.join("machine.yaml");
        let user_a = dir.join("a.yaml");
        let user_b = dir.join("b.yaml");
        fs::write(
            &machine,
            "priority:\n  ffmpeg:\n    - Gyan.FFmpeg\nforce_exclude: []\nforce_include: []\n",
        )
        .unwrap();
        fs::write(
            &user_a,
            "priority:\n  ffmpeg:\n    - UserA.Build\nforce_include:\n  - id: UserA.Tool\n    commands: [usera]\nforce_exclude: []\n",
        )
        .unwrap();
        fs::write(
            &user_b,
            "priority:\n  ffmpeg:\n    - UserB.Build\nforce_include:\n  - id: UserB.Tool\n    commands: [userb]\nforce_exclude: []\n",
        )
        .unwrap();

        let a = load_layered(Some(&machine), Some(&user_a)).unwrap();
        let b = load_layered(Some(&machine), Some(&user_b)).unwrap();
        let machine_only = load_layered(Some(&machine), None).unwrap();

        assert_eq!(a.rank("ffmpeg", "UserA.Build").0, 0);
        assert_eq!(b.rank("ffmpeg", "UserB.Build").0, 0);
        assert_ne!(
            a.rank("ffmpeg", "UserA.Build"),
            b.rank("ffmpeg", "UserA.Build")
        );
        assert!(a.force_include.iter().any(|item| item.id == "UserA.Tool"));
        assert!(b.force_include.iter().all(|item| item.id != "UserA.Tool"));
        assert!(machine_only
            .force_include
            .iter()
            .all(|item| item.id != "UserA.Tool" && item.id != "UserB.Tool"));
        assert_eq!(machine_only.rank("ffmpeg", "Gyan.FFmpeg").0, 0);
    }
}
