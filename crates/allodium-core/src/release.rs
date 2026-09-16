use crate::ValidationReport;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const RELEASE_SCHEMA_V0: &str = "allodium.release/v0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseRecord {
    pub schema: String,
    pub id: String,
    pub title: String,
    pub version: String,
    pub state: String,
    pub revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalRelease {
    pub record: ReleaseRecord,
    pub notes: String,
    pub directory: PathBuf,
}

pub fn load_releases(root: impl AsRef<Path>) -> Result<Vec<CanonicalRelease>, String> {
    let releases_dir = root.as_ref().join(".project/releases");
    if !releases_dir.exists() {
        return Ok(Vec::new());
    }
    if !releases_dir.is_dir() {
        return Err(format!("{}: expected a directory", releases_dir.display()));
    }

    let mut directories = fs::read_dir(&releases_dir)
        .map_err(|error| format!("{}: {error}", releases_dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("{}: {error}", releases_dir.display()))?;
    directories.sort_by_key(|entry| entry.file_name());

    let mut releases = Vec::new();
    for entry in directories {
        let directory = entry.path();
        if !directory.is_dir() {
            continue;
        }
        let record_path = directory.join("release.toml");
        let notes_path = directory.join("notes.md");
        let text = fs::read_to_string(&record_path)
            .map_err(|error| format!("{}: {error}", record_path.display()))?;
        let record: ReleaseRecord = toml::from_str(&text)
            .map_err(|error| format!("{}: {error}", record_path.display()))?;
        let notes = fs::read_to_string(&notes_path)
            .map_err(|error| format!("{}: {error}", notes_path.display()))?;
        releases.push(CanonicalRelease {
            record,
            notes,
            directory,
        });
    }
    releases.sort_by(|left, right| left.record.id.cmp(&right.record.id));
    Ok(releases)
}

pub(crate) fn validate_releases(root: &Path, report: &mut ValidationReport) {
    let releases = match load_releases(root) {
        Ok(releases) => releases,
        Err(error) => {
            report.errors.push(error);
            return;
        }
    };

    let mut versions = BTreeSet::new();
    let mut tags = BTreeSet::new();
    for release in releases {
        let record_path = release.directory.join("release.toml");
        let expected_id = release
            .directory
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();

        if release.record.schema != RELEASE_SCHEMA_V0 {
            report.errors.push(format!(
                "{}: unsupported schema {:?}; expected {:?}",
                record_path.display(),
                release.record.schema,
                RELEASE_SCHEMA_V0
            ));
        }
        if release.record.id != expected_id {
            report.errors.push(format!(
                "{}: id {:?} must match directory {:?}",
                record_path.display(),
                release.record.id,
                expected_id
            ));
        }
        if release.record.title.trim().is_empty() {
            report.errors.push(format!(
                "{}: title must not be empty",
                record_path.display()
            ));
        }
        if release.record.version.trim().is_empty() {
            report.errors.push(format!(
                "{}: version must not be empty",
                record_path.display()
            ));
        } else if !versions.insert(release.record.version.clone()) {
            report.errors.push(format!(
                "{}: duplicate canonical release version {:?}",
                record_path.display(),
                release.record.version
            ));
        }
        if !matches!(release.record.state.as_str(), "draft" | "published") {
            report.errors.push(format!(
                "{}: state must be draft or published",
                record_path.display()
            ));
        }
        if release.record.revision.trim().is_empty() {
            report.errors.push(format!(
                "{}: revision must not be empty",
                record_path.display()
            ));
        }
        if let Some(tag) = &release.record.tag {
            if tag.trim().is_empty() {
                report.errors.push(format!(
                    "{}: tag must not be empty when present",
                    record_path.display()
                ));
            } else if !tags.insert(tag.clone()) {
                report.errors.push(format!(
                    "{}: duplicate canonical release tag {:?}",
                    record_path.display(),
                    tag
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn validator_accepts_release() {
        let root = test_root("valid");
        write_release(&root, "release-0001", "0.1.0", Some("v0.1.0"));

        let report = crate::validate(&root);
        assert!(report.is_ok(), "{:?}", report.errors);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validator_rejects_release_identity_mismatch() {
        let root = test_root("mismatch");
        write_release(&root, "release-0001", "0.1.0", Some("v0.1.0"));
        fs::write(
            root.join(".project/releases/release-0001/release.toml"),
            release_record("release-9999", "0.1.0", Some("v0.1.0")),
        )
        .unwrap();

        let report = crate::validate(&root);
        assert!(!report.is_ok());
        assert!(report.errors.iter().any(|error| error.contains("must match directory")));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validator_rejects_duplicate_release_tags() {
        let root = test_root("duplicate-tag");
        write_release(&root, "release-0001", "0.1.0", Some("v0.1.0"));
        write_release(&root, "release-0002", "0.2.0", Some("v0.1.0"));

        let report = crate::validate(&root);
        assert!(!report.is_ok());
        assert!(report.errors.iter().any(|error| error.contains("duplicate canonical release tag")));

        fs::remove_dir_all(root).unwrap();
    }

    fn write_release(root: &Path, id: &str, version: &str, tag: Option<&str>) {
        let directory = root.join(".project/releases").join(id);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("release.toml"), release_record(id, version, tag)).unwrap();
        fs::write(directory.join("notes.md"), "Release notes\n").unwrap();
    }

    fn release_record(id: &str, version: &str, tag: Option<&str>) -> String {
        let tag = tag
            .map(|tag| format!("tag = \"{tag}\"\n"))
            .unwrap_or_default();
        format!(
            "schema = \"allodium.release/v0\"\nid = \"{id}\"\ntitle = \"{version}\"\nversion = \"{version}\"\nstate = \"draft\"\nrevision = \"0123456789abcdef0123456789abcdef01234567\"\n{tag}"
        )
    }

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-release-{name}-{}-{nonce}",
            std::process::id()
        ));
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
        root
    }
}
