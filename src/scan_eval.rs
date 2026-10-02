use std::collections::BTreeSet;

use serde_yaml::Value;

use crate::catalog::{is_self_package, PackageEntry};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evaluated {
    pub id: String,
    pub version: String,
    pub commands: BTreeSet<String>,
    pub archive_binaries_depend_on_path: bool,
}

/// Include the latest manifest when an installer is portable, or a zip whose nested
/// installer is portable, and Commands is non-empty. PortableCommandAlias alone does
/// not count: Gyan.FFmpeg is force-included for that case.
pub fn evaluate_manifest(yaml_text: &str) -> Option<Evaluated> {
    let value: Value = serde_yaml::from_str(yaml_text).ok()?;
    let id = string_field(&value, "PackageIdentifier")?;
    if is_self_package(&id) {
        return None;
    }
    let version = string_field(&value, "PackageVersion").unwrap_or_else(|| "unknown".into());
    let root_type = string_field(&value, "InstallerType");
    let root_nested = string_field(&value, "NestedInstallerType");
    let root_commands = string_list(&value, "Commands");
    let root_depend = bool_field(&value, "ArchiveBinariesDependOnPath");

    let installers = value
        .get("Installers")
        .and_then(Value::as_sequence)
        .cloned()
        .unwrap_or_else(|| vec![Value::Mapping(Default::default())]);

    let mut commands = BTreeSet::new();
    let mut depend = false;
    let mut matched = false;
    for installer in &installers {
        let kind = string_field(installer, "InstallerType").or_else(|| root_type.clone());
        let nested = string_field(installer, "NestedInstallerType").or_else(|| root_nested.clone());
        if !installer_matches(kind.as_deref(), nested.as_deref()) {
            continue;
        }
        let own = string_list(installer, "Commands");
        let effective = if own.is_empty() {
            root_commands.clone()
        } else {
            own
        };
        if effective.is_empty() {
            continue;
        }
        matched = true;
        let installer_depend =
            bool_field(installer, "ArchiveBinariesDependOnPath").unwrap_or(false);
        if installer_depend || root_depend.unwrap_or(false) {
            depend = true;
        }
        commands.extend(effective);
    }
    if !matched {
        return None;
    }
    Some(Evaluated {
        id,
        version,
        commands,
        archive_binaries_depend_on_path: depend,
    })
}

pub fn manifest_version(yaml_text: &str) -> Option<String> {
    let value: Value = serde_yaml::from_str(yaml_text).ok()?;
    string_field(&value, "PackageVersion")
}

pub fn manifest_depend_flag(yaml_text: &str) -> bool {
    let Ok(value) = serde_yaml::from_str::<Value>(yaml_text) else {
        return false;
    };
    if bool_field(&value, "ArchiveBinariesDependOnPath").unwrap_or(false) {
        return true;
    }
    value
        .get("Installers")
        .and_then(Value::as_sequence)
        .map(|installers| {
            installers.iter().any(|installer| {
                bool_field(installer, "ArchiveBinariesDependOnPath").unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn installer_matches(kind: Option<&str>, nested: Option<&str>) -> bool {
    let kind = kind.unwrap_or("").trim();
    let nested = nested.unwrap_or("").trim();
    kind.eq_ignore_ascii_case("portable")
        || (kind.eq_ignore_ascii_case("zip") && nested.eq_ignore_ascii_case("portable"))
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn bool_field(value: &Value, key: &str) -> Option<bool> {
    match value.get(key)? {
        Value::Bool(flag) => Some(*flag),
        Value::String(text) => match text.to_ascii_lowercase().as_str() {
            "true" | "yes" => Some(true),
            "false" | "no" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn string_list(value: &Value, key: &str) -> Vec<String> {
    match value.get(key) {
        Some(Value::Sequence(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect(),
        Some(Value::String(item)) => vec![item.trim().to_string()],
        _ => Vec::new(),
    }
}

impl Evaluated {
    pub fn into_package(self) -> PackageEntry {
        let mut commands: Vec<String> = self.commands.into_iter().collect();
        commands.sort();
        PackageEntry {
            id: self.id,
            version: self.version,
            commands,
            archive_binaries_depend_on_path: self.archive_binaries_depend_on_path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_and_zip_nested_portable_need_commands() {
        let portable = r#"
PackageIdentifier: Example.Portable
PackageVersion: 1.2.3
Installers:
  - Architecture: x64
    InstallerType: portable
    Commands: [hello]
"#;
        let parsed = evaluate_manifest(portable).unwrap();
        assert_eq!(parsed.commands.iter().next().unwrap(), "hello");

        let zipped = r#"
PackageIdentifier: Example.Zip
PackageVersion: 2
InstallerType: zip
NestedInstallerType: portable
Commands: [tool]
ArchiveBinariesDependOnPath: true
Installers:
  - Architecture: x64
    InstallerUrl: https://example.invalid/tool.zip
"#;
        let parsed = evaluate_manifest(zipped).unwrap();
        assert!(parsed.archive_binaries_depend_on_path);
        assert!(parsed.commands.contains("tool"));

        let alias_only = r#"
PackageIdentifier: Gyan.FFmpeg
PackageVersion: 9.0.2
InstallerType: zip
NestedInstallerType: portable
NestedInstallerFiles:
  - RelativeFilePath: ffmpeg.exe
    PortableCommandAlias: ffmpeg
Installers:
  - Architecture: x64
    InstallerUrl: https://example.invalid/ffmpeg.zip
"#;
        assert!(evaluate_manifest(alias_only).is_none());

        let msi = r#"
PackageIdentifier: Example.Msi
PackageVersion: 1
Installers:
  - InstallerType: wix
    Commands: [widget]
"#;
        assert!(evaluate_manifest(msi).is_none());

        let empty = r#"
PackageIdentifier: Example.Empty
PackageVersion: 1
Installers:
  - InstallerType: portable
"#;
        assert!(evaluate_manifest(empty).is_none());

        let nested_exe = r#"
PackageIdentifier: Example.NestedExe
PackageVersion: 1
InstallerType: zip
NestedInstallerType: exe
Commands: [widget]
Installers:
  - Architecture: x64
"#;
        assert!(evaluate_manifest(nested_exe).is_none());
    }

    #[test]
    fn self_package_is_excluded() {
        let yaml = r#"
PackageIdentifier: Clevotec.CleverShim
PackageVersion: 0.1.0
Installers:
  - InstallerType: portable
    Commands: [clevershim]
"#;
        assert!(evaluate_manifest(yaml).is_none());
    }
}
