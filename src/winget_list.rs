use std::process::Command;

pub const WINGET_NOT_FOUND: i32 = -1978335228;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WingetList {
    Installed { version: String },
    NotInstalled,
    Failed { message: String },
}

impl WingetList {
    pub fn summary(&self) -> String {
        match self {
            WingetList::Installed { version } => format!("installed {version}"),
            WingetList::NotInstalled => "not installed".into(),
            WingetList::Failed { message } => format!("failed: {message}"),
        }
    }
}

pub fn query_winget(package_id: &str) -> WingetList {
    let program = std::env::var("CLEVERSHIM_WINGET").unwrap_or_else(|_| "winget".into());
    let output = Command::new(&program)
        .args([
            "list",
            "--id",
            package_id,
            "--accept-source-agreements",
            "--disable-interactivity",
        ])
        .output();
    match output {
        Ok(output) => {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            parse_winget_list(&text, output.status.code(), package_id)
        }
        Err(err) => WingetList::Failed {
            message: format!("could not run {program}: {err}"),
        },
    }
}

pub fn parse_winget_list(output: &str, exit_code: Option<i32>, package_id: &str) -> WingetList {
    let folded = output.to_ascii_lowercase();
    let not_found_text = folded.contains("no installed package found");
    if not_found_text || exit_code == Some(WINGET_NOT_FOUND) {
        return WingetList::NotInstalled;
    }
    if exit_code.is_some() && exit_code != Some(0) {
        return WingetList::Failed {
            message: one_line(output),
        };
    }
    if let Some(version) = version_on_matching_line(output, package_id) {
        return WingetList::Installed { version };
    }
    if exit_code == Some(0) {
        WingetList::NotInstalled
    } else {
        WingetList::Failed {
            message: one_line(output),
        }
    }
}

fn version_on_matching_line(output: &str, package_id: &str) -> Option<String> {
    for line in output.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let Some(index) = tokens
            .iter()
            .position(|token| token.eq_ignore_ascii_case(package_id))
        else {
            continue;
        };
        let version = tokens.get(index + 1).copied().unwrap_or("unknown");
        if version.eq_ignore_ascii_case("version") {
            continue;
        }
        return Some(version.to_string());
    }
    None
}

fn one_line(output: &str) -> String {
    let text = output.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        "winget exited with an error".into()
    } else if text.len() > 400 {
        format!("{}…", &text[..400])
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_installed_not_installed_and_failed_query() {
        let table = "Name Id Version Source\nFFmpeg Gyan.FFmpeg 9.0.2 winget\n";
        assert_eq!(
            parse_winget_list(table, Some(0), "Gyan.FFmpeg"),
            WingetList::Installed {
                version: "9.0.2".into()
            }
        );
        assert_eq!(
            parse_winget_list(
                "No installed package found matching input criteria.",
                Some(WINGET_NOT_FOUND),
                "Gone.Pkg"
            ),
            WingetList::NotInstalled
        );
        assert_eq!(
            parse_winget_list("Failed when opening source", Some(1), "Gyan.FFmpeg"),
            WingetList::Failed {
                message: "Failed when opening source".into()
            }
        );
    }
}
