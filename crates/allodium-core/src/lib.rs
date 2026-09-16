use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const PROJECT_SCHEMA_V0: &str = "allodium.project/v0";
pub const ISSUE_SCHEMA_V0: &str = "allodium.issue/v0";
pub const REMOTE_SCHEMA_V0: &str = "allodium.remote/v0";

#[derive(Debug, Deserialize)]
pub struct ProjectManifest {
    pub schema: String,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct IssueRecord {
    pub schema: String,
    pub id: String,
    pub title: String,
    pub state: String,
    #[serde(default)]
    pub labels: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct RemoteRecord {
    pub schema: String,
    pub name: String,
    pub kind: String,
    pub repository: String,
    pub outbound: String,
    pub inbound: String,
}

#[derive(Debug, Default)]
pub struct ValidationReport {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl ValidationReport {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}

pub fn load_manifest(root: impl AsRef<Path>) -> Result<ProjectManifest, String> {
    let path = root.as_ref().join(".project/manifest.toml");
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn validate(root: impl AsRef<Path>) -> ValidationReport {
    let root = root.as_ref();
    let mut report = ValidationReport::default();

    match load_manifest(root) {
        Ok(manifest) => {
            if manifest.schema != PROJECT_SCHEMA_V0 {
                report.errors.push(format!(
                    ".project/manifest.toml: unsupported schema {:?}; expected {:?}",
                    manifest.schema, PROJECT_SCHEMA_V0
                ));
            }
            if manifest.id.trim().is_empty() {
                report.errors.push(".project/manifest.toml: id must not be empty".into());
            }
            if manifest.name.trim().is_empty() {
                report.errors.push(".project/manifest.toml: name must not be empty".into());
            }
        }
        Err(error) => report.errors.push(error),
    }

    validate_issues(root, &mut report);
    validate_remotes(root, &mut report);
    report
}

fn validate_issues(root: &Path, report: &mut ValidationReport) {
    let issues = root.join(".project/issues");
    for dir in child_directories(&issues, report) {
        let expected_id = dir.file_name().and_then(|s| s.to_str()).unwrap_or_default();
        let record_path = dir.join("issue.toml");
        let text = match fs::read_to_string(&record_path) {
            Ok(text) => text,
            Err(error) => {
                report.errors.push(format!("{}: {error}", record_path.display()));
                continue;
            }
        };
        let issue: IssueRecord = match toml::from_str(&text) {
            Ok(issue) => issue,
            Err(error) => {
                report.errors.push(format!("{}: {error}", record_path.display()));
                continue;
            }
        };
        if issue.schema != ISSUE_SCHEMA_V0 {
            report.errors.push(format!("{}: unsupported schema {:?}", record_path.display(), issue.schema));
        }
        if issue.id != expected_id {
            report.errors.push(format!(
                "{}: id {:?} must match directory {:?}",
                record_path.display(), issue.id, expected_id
            ));
        }
        if issue.title.trim().is_empty() {
            report.errors.push(format!("{}: title must not be empty", record_path.display()));
        }
        if !matches!(issue.state.as_str(), "open" | "closed") {
            report.errors.push(format!("{}: state must be open or closed", record_path.display()));
        }
    }
}

fn validate_remotes(root: &Path, report: &mut ValidationReport) {
    let remotes = root.join(".project/remotes");
    for dir in child_directories(&remotes, report) {
        let expected_name = dir.file_name().and_then(|s| s.to_str()).unwrap_or_default();
        let record_path = dir.join("remote.toml");
        let text = match fs::read_to_string(&record_path) {
            Ok(text) => text,
            Err(error) => {
                report.errors.push(format!("{}: {error}", record_path.display()));
                continue;
            }
        };
        let remote: RemoteRecord = match toml::from_str(&text) {
            Ok(remote) => remote,
            Err(error) => {
                report.errors.push(format!("{}: {error}", record_path.display()));
                continue;
            }
        };
        if remote.schema != REMOTE_SCHEMA_V0 {
            report.errors.push(format!("{}: unsupported schema {:?}", record_path.display(), remote.schema));
        }
        if remote.name != expected_name {
            report.errors.push(format!(
                "{}: name {:?} must match directory {:?}",
                record_path.display(), remote.name, expected_name
            ));
        }
        if remote.kind.trim().is_empty() || remote.repository.trim().is_empty() {
            report.errors.push(format!("{}: kind and repository are required", record_path.display()));
        }
    }
}

fn child_directories(path: &Path, report: &mut ValidationReport) -> Vec<PathBuf> {
    if !path.exists() {
        return Vec::new();
    }
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) => {
            report.errors.push(format!("{}: {error}", path.display()));
            return Vec::new();
        }
    };
    let mut dirs = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) if entry.path().is_dir() => dirs.push(entry.path()),
            Ok(_) => {}
            Err(error) => report.errors.push(format!("{}: {error}", path.display())),
        }
    }
    dirs.sort();
    dirs
}
