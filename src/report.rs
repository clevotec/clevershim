#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateNote {
    pub path: String,
    pub disposition: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairReport {
    pub shim: String,
    pub scope: String,
    pub package_id: String,
    pub old_target: String,
    pub old_target_exists: bool,
    pub winget_list: String,
    pub searched: Vec<String>,
    pub candidates: Vec<CandidateNote>,
    pub sidecar: String,
    pub removed_shims: Vec<String>,
    pub next: String,
    pub success: bool,
    pub removed: bool,
}

impl RepairReport {
    pub fn render(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("shim: {}", self.shim));
        lines.push(format!("scope: {}", self.scope));
        lines.push(format!("package: {}", self.package_id));
        lines.push(format!("old_target: {}", self.old_target));
        lines.push(format!(
            "old_target_exists: {}",
            if self.old_target_exists { "yes" } else { "no" }
        ));
        lines.push(format!("winget_list: {}", self.winget_list));
        if self.searched.is_empty() {
            lines.push("searched: (none)".into());
        } else {
            for place in &self.searched {
                lines.push(format!("searched: {place}"));
            }
        }
        if self.candidates.is_empty() {
            lines.push("candidate: (none)".into());
        } else {
            for candidate in &self.candidates {
                lines.push(format!(
                    "candidate: {} | {}",
                    candidate.path, candidate.disposition
                ));
            }
        }
        lines.push(format!("sidecar: {}", self.sidecar));
        if self.removed_shims.is_empty() {
            lines.push("removed_shims: (none)".into());
        } else {
            lines.push(format!("removed_shims: {}", self.removed_shims.join(", ")));
            lines.push(format!("{} is no longer installed", self.shim));
        }
        lines.push(format!("next: {}", self.next));
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_report_names_the_debug_fields() {
        let report = RepairReport {
            shim: "ffmpeg".into(),
            scope: "user".into(),
            package_id: "Gyan.FFmpeg".into(),
            old_target: r"C:\old\ffmpeg.exe".into(),
            old_target_exists: false,
            winget_list: "installed 9.0.2".into(),
            searched: vec![
                "links | C:\\Links".into(),
                "package-folders | C:\\Packages".into(),
            ],
            candidates: vec![CandidateNote {
                path: r"C:\old\ffmpeg.exe".into(),
                disposition: "missing".into(),
            }],
            sidecar: "unchanged".into(),
            removed_shims: vec![],
            next: "clevershim list".into(),
            success: false,
            removed: false,
        };
        let text = report.render();
        for needle in [
            "shim: ffmpeg",
            "scope: user",
            "package: Gyan.FFmpeg",
            "old_target: C:\\old\\ffmpeg.exe",
            "old_target_exists: no",
            "winget_list: installed 9.0.2",
            "searched: links",
            "candidate: C:\\old\\ffmpeg.exe | missing",
            "sidecar: unchanged",
            "next: clevershim list",
        ] {
            assert!(text.contains(needle), "{text} missing {needle}");
        }
        assert!(text.ends_with("next: clevershim list"));
    }
}
