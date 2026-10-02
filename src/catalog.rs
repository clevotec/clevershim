use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::overrides::Overrides;

pub const EMBEDDED_CATALOG: &str = include_str!("../catalog/packages.yaml");
pub const EMBEDDED_OVERRIDES: &str = include_str!("../catalog/overrides.yaml");
pub const SELF_PACKAGE_ID: &str = "Clevotec.CleverShim";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageEntry {
    pub id: String,
    pub version: String,
    pub commands: Vec<String>,
    #[serde(default)]
    pub archive_binaries_depend_on_path: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CatalogFile {
    #[serde(default)]
    pub packages: Vec<PackageEntry>,
    #[serde(default)]
    pub collisions: BTreeMap<String, Vec<String>>,
}

impl CatalogFile {
    pub fn parse(text: &str) -> Result<Self, String> {
        serde_yaml::from_str(text).map_err(|err| err.to_string())
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path).map_err(|err| err.to_string())?;
        Self::parse(&text)
    }

    pub fn embedded() -> Result<Self, String> {
        if let Ok(path) = std::env::var("CLEVERSHIM_CATALOG") {
            return Self::load(Path::new(&path));
        }
        Self::parse(EMBEDDED_CATALOG)
    }

    pub fn get(&self, id: &str) -> Option<&PackageEntry> {
        self.packages
            .iter()
            .find(|package| package.id.eq_ignore_ascii_case(id))
    }

    pub fn providers_for<'a>(&'a self, command: &str) -> Vec<&'a PackageEntry> {
        self.packages
            .iter()
            .filter(|package| {
                package
                    .commands
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(command))
            })
            .collect()
    }
}

pub fn is_self_package(id: &str) -> bool {
    id.eq_ignore_ascii_case(SELF_PACKAGE_ID)
}

/// Drop refused packages, add force-includes, and record command collisions.
pub fn finalize_catalog(mut packages: Vec<PackageEntry>, overrides: &Overrides) -> CatalogFile {
    packages.retain(|package| {
        !is_self_package(&package.id) && !overrides.is_force_excluded(&package.id)
    });

    for include in &overrides.force_include {
        if is_self_package(&include.id) || overrides.is_force_excluded(&include.id) {
            continue;
        }
        if let Some(existing) = packages
            .iter_mut()
            .find(|package| package.id.eq_ignore_ascii_case(&include.id))
        {
            for command in &include.commands {
                if !existing
                    .commands
                    .iter()
                    .any(|have| have.eq_ignore_ascii_case(command))
                {
                    existing.commands.push(command.clone());
                }
            }
            existing.commands.sort();
        } else {
            let mut commands = include.commands.clone();
            commands.sort();
            commands.dedup();
            packages.push(PackageEntry {
                id: include.id.clone(),
                version: include.version.clone().unwrap_or_else(|| "unknown".into()),
                commands,
                archive_binaries_depend_on_path: include.archive_binaries_depend_on_path,
            });
        }
    }

    packages.sort_by(|left, right| {
        left.id
            .to_ascii_lowercase()
            .cmp(&right.id.to_ascii_lowercase())
    });
    for package in &mut packages {
        package.commands.sort();
        package.commands.dedup();
    }

    let mut owners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut display: BTreeMap<String, String> = BTreeMap::new();
    for package in &packages {
        for command in &package.commands {
            let key = command.to_ascii_lowercase();
            display
                .entry(key.clone())
                .or_insert_with(|| command.clone());
            owners.entry(key).or_default().insert(package.id.clone());
        }
    }
    let mut collisions = BTreeMap::new();
    for (key, ids) in owners {
        if ids.len() < 2 {
            continue;
        }
        let mut list: Vec<String> = ids.into_iter().collect();
        list.sort();
        collisions.insert(display.remove(&key).unwrap_or(key), list);
    }

    CatalogFile {
        packages,
        collisions,
    }
}

pub fn merge_packages(base: &CatalogFile, extra: &CatalogFile) -> Vec<PackageEntry> {
    let mut by_id: BTreeMap<String, PackageEntry> = BTreeMap::new();
    for package in base.packages.iter().chain(extra.packages.iter()) {
        by_id.insert(package.id.to_ascii_lowercase(), package.clone());
    }
    by_id.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overrides::ForceInclude;

    #[test]
    fn embedded_catalog_parses_and_skips_self() {
        let catalog = CatalogFile::parse(EMBEDDED_CATALOG).unwrap();
        assert!(catalog
            .packages
            .iter()
            .all(|package| !is_self_package(&package.id)));
        if catalog.packages.is_empty() {
            return;
        }
        let gyan = catalog
            .get("Gyan.FFmpeg")
            .expect("force-include Gyan.FFmpeg");
        for command in ["ffmpeg", "ffprobe", "ffplay"] {
            assert!(
                gyan.commands.iter().any(|name| name == command),
                "missing {command}"
            );
        }
    }

    #[test]
    fn force_include_adds_ffmpeg_and_records_collisions() {
        let overrides = Overrides {
            priority: BTreeMap::new(),
            force_include: vec![ForceInclude {
                id: "Gyan.FFmpeg".into(),
                commands: vec!["ffmpeg".into(), "ffprobe".into()],
                version: Some("9.0.2".into()),
                archive_binaries_depend_on_path: false,
            }],
            force_exclude: BTreeSet::from(["Clevotec.CleverShim".into()]),
        };
        let found = vec![
            PackageEntry {
                id: "yt-dlp.FFmpeg".into(),
                version: "1".into(),
                commands: vec!["ffmpeg".into()],
                archive_binaries_depend_on_path: true,
            },
            PackageEntry {
                id: "Clevotec.CleverShim".into(),
                version: "0.1.0".into(),
                commands: vec!["clevershim".into()],
                archive_binaries_depend_on_path: false,
            },
        ];
        let catalog = finalize_catalog(found, &overrides);
        assert!(catalog.get("Clevotec.CleverShim").is_none());
        let gyan = catalog.get("Gyan.FFmpeg").unwrap();
        assert_eq!(
            gyan.commands,
            vec!["ffmpeg".to_string(), "ffprobe".to_string()]
        );
        assert!(catalog.collisions.get("ffmpeg").unwrap().len() >= 2);
    }
}
