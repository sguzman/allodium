pub mod github;

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const PROJECT_SCHEMA_V0: &str = "allodium.project/v0";
pub const ISSUE_SCHEMA_V0: &str = "allodium.issue/v0";
pub const REMOTE_SCHEMA_V0: &str = "allodium.remote/v0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectManifest {
    pub schema: String,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IssueRecord {
    pub schema: String,
    pub id: String,
    pub title: String,
    pub state: String,
    #[serde(default)]
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalIssue {
    pub record: IssueRecord,
    pub body: String,
    pub directory: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    read_toml(&path)
}

pub fn load_remote(root: impl AsRef<Path>, name: &str) -> Result<RemoteRecord, String> {
    let path = root
        .as_ref()
        .join(".project/remotes")
        .join(name)
        .join("remote.toml");
    read_toml(&path)
}

pub fn load_issues(root: impl AsRef<Path>) -> Result<Vec<CanonicalIssue>, String> {
    let root = root.as_ref();
    let issues_dir = root.join(".project/issues");
    let mut issues = Vec::new();

    for directory in child_directories(&issues_dir)? {
        let record_path = directory.join("issue.toml");
        let body_path = directory.join("body.md");
        let record: IssueRecord = read_toml(&record_path)?;
        let body = fs::read_to_string(&body_path)
            .map_err(|error| format!("{}: {error}", body_path.display()))?;
        issues.push(CanonicalIssue {
            record,
            body,
            directory,
        });
    }

    issues.sort_by(|left, right| left.record.id.cmp(&right.record.id));
    Ok(issues)
}

pub fn validate(root: impl AsRef<Path>) -> ValidationReport {
    let root = root.as_ref();
    let mut report = ValidationReport::default();

    match load_manifest(root) {
        Ok(manifest) => validate_manifest(&manifest, &mut report),
        Err(error) => report.errors.push(error),
    }

    match load_issues(root) {
        Ok(issues) => {
            for issue in &issues {
                validate_issue(issue, &mut report);
            }
        }
        Err(error) => report.errors.push(error),
    }

    validate_remotes(root, &mut report);
    report
}

fn validate_manifest(manifest: &ProjectManifest, report: &mut ValidationReport) {
    if manifest.schema != PROJECT_SCHEMA_V0 {
        report.errors.push(format!(
            ".project/manifest.toml: unsupported schema {:?}; expected {:?}",
            manifest.schema, PROJECT_SCHEMA_V0
        ));
    }
    if manifest.id.trim().is_empty() {
        report
            .errors
            .push(".project/manifest.toml: id must not be empty".into());
    }
    if manifest.name.trim().is_empty() {
        report
            .errors
            .push(".project/manifest.toml: name must not be empty".into());
    }
}

fn validate_issue(issue: &CanonicalIssue, report: &mut ValidationReport) {
    let record_path = issue.directory.join("issue.toml");
    let expected_id = issue
        .directory
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    if issue.record.schema != ISSUE_SCHEMA_V0 {
        report.errors.push(format!(
            "{}: unsupported schema {:?}",
            record_path.display(),
            issue.record.schema
        ));
    }
    if issue.record.id != expected_id {
        report.errors.push(format!(
            "{}: id {:?} must match directory {:?}",
            record_path.display(),
            issue.record.id,
            expected_id
        ));
    }
    if issue.record.title.trim().is_empty() {
        report.errors.push(format!(
            "{}: title must not be empty",
            record_path.display()
        ));
    }
    if !matches!(issue.record.state.as_str(), "open" | "closed") {
        report.errors.push(format!(
            "{}: state must be open or closed",
            record_path.display()
        ));
    }
}

fn validate_remotes(root: &Path, report: &mut ValidationReport) {
    let remotes = root.join(".project/remotes");
    let directories = match child_directories(&remotes) {
        Ok(directories) => directories,
        Err(error) => {
            report.errors.push(error);
            return;
        }
    };

    for directory in directories {
        let expected_name = directory
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let record_path = directory.join("remote.toml");
        let remote: RemoteRecord = match read_toml(&record_path) {
            Ok(remote) => remote,
            Err(error) => {
                report.errors.push(error);
                continue;
            }
        };

        if remote.schema != REMOTE_SCHEMA_V0 {
            report.errors.push(format!(
                "{}: unsupported schema {:?}",
                record_path.display(),
                remote.schema
            ));
        }
        if remote.name != expected_name {
            report.errors.push(format!(
                "{}: name {:?} must match directory {:?}",
                record_path.display(),
                remote.name,
                expected_name
            ));
        }
        if remote.kind.trim().is_empty() || remote.repository.trim().is_empty() {
            report.errors.push(format!(
                "{}: kind and repository are required",
                record_path.display()
            ));
        }
    }
}

fn read_toml<T>(path: &Path) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let text = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))
}

fn child_directories(path: &Path) -> Result<Vec<PathBuf>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut directories = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("{}: {error}", path.display()))?;
        if entry.path().is_dir() {
            directories.push(entry.path());
        }
    }
    directories.sort();
    Ok(directories)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn validator_accepts_minimal_project() {
        let root = test_root("valid");
        write_minimal_project(&root);

        let report = validate(&root);
        assert!(report.is_ok(), "{:?}", report.errors);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validator_rejects_issue_directory_identity_mismatch() {
        let root = test_root("mismatch");
        write_minimal_project(&root);
        fs::write(
            root.join(".project/issues/issue-0001/issue.toml"),
            "schema = \"allodium.issue/v0\"\nid = \"issue-9999\"\ntitle = \"Test\"\nstate = \"open\"\n",
        )
        .unwrap();

        let report = validate(&root);
        assert!(!report.is_ok());
        assert!(report.errors.iter().any(|error| error.contains("must match directory")));

        fs::remove_dir_all(root).unwrap();
    }

    fn write_minimal_project(root: &Path) {
        fs::create_dir_all(root.join(".project/issues/issue-0001")).unwrap();
        fs::create_dir_all(root.join(".project/remotes/github")).unwrap();
        fs::write(
            root.join(".project/manifest.toml"),
            "schema = \"allodium.project/v0\"\nid = \"test\"\nname = \"Test\"\n",
        )
        .unwrap();
        fs::write(
            root.join(".project/issues/issue-0001/issue.toml"),
            "schema = \"allodium.issue/v0\"\nid = \"issue-0001\"\ntitle = \"Test\"\nstate = \"open\"\n",
        )
        .unwrap();
        fs::write(root.join(".project/issues/issue-0001/body.md"), "Body\n").unwrap();
        fs::write(
            root.join(".project/remotes/github/remote.toml"),
            "schema = \"allodium.remote/v0\"\nname = \"github\"\nkind = \"github\"\nrepository = \"owner/repo\"\noutbound = \"reconcile\"\ninbound = \"archive\"\n",
        )
        .unwrap();
    }

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("allodium-{name}-{}-{nonce}", std::process::id()))
    }
}
