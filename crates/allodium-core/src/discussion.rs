use crate::ValidationReport;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const DISCUSSION_SCHEMA_V0: &str = "allodium.discussion/v0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscussionRecord {
    pub schema: String,
    pub id: String,
    pub title: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalDiscussion {
    pub record: DiscussionRecord,
    pub body: String,
    pub directory: PathBuf,
}

pub fn load_discussions(root: impl AsRef<Path>) -> Result<Vec<CanonicalDiscussion>, String> {
    let discussions_dir = root.as_ref().join(".project/discussions");
    if !discussions_dir.exists() {
        return Ok(Vec::new());
    }
    if !discussions_dir.is_dir() {
        return Err(format!(
            "{}: expected a directory",
            discussions_dir.display()
        ));
    }

    let mut entries = fs::read_dir(&discussions_dir)
        .map_err(|error| format!("{}: {error}", discussions_dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("{}: {error}", discussions_dir.display()))?;
    entries.sort_by_key(|entry| entry.file_name());

    let mut discussions = Vec::new();
    for entry in entries {
        let directory = entry.path();
        if !directory.is_dir() {
            continue;
        }

        let record_path = directory.join("discussion.toml");
        let body_path = directory.join("body.md");
        let text = fs::read_to_string(&record_path)
            .map_err(|error| format!("{}: {error}", record_path.display()))?;
        let record: DiscussionRecord =
            toml::from_str(&text).map_err(|error| format!("{}: {error}", record_path.display()))?;
        let body = fs::read_to_string(&body_path)
            .map_err(|error| format!("{}: {error}", body_path.display()))?;
        discussions.push(CanonicalDiscussion {
            record,
            body,
            directory,
        });
    }

    discussions.sort_by(|left, right| left.record.id.cmp(&right.record.id));
    Ok(discussions)
}

pub(crate) fn validate_discussions(root: &Path, report: &mut ValidationReport) {
    let discussions = match load_discussions(root) {
        Ok(discussions) => discussions,
        Err(error) => {
            report.errors.push(error);
            return;
        }
    };

    for discussion in discussions {
        let record_path = discussion.directory.join("discussion.toml");
        let expected_id = discussion
            .directory
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();

        if discussion.record.schema != DISCUSSION_SCHEMA_V0 {
            report.errors.push(format!(
                "{}: unsupported schema {:?}; expected {:?}",
                record_path.display(),
                discussion.record.schema,
                DISCUSSION_SCHEMA_V0
            ));
        }
        if discussion.record.id != expected_id {
            report.errors.push(format!(
                "{}: id {:?} must match directory {:?}",
                record_path.display(),
                discussion.record.id,
                expected_id
            ));
        }
        if discussion.record.title.trim().is_empty() {
            report.errors.push(format!(
                "{}: title must not be empty",
                record_path.display()
            ));
        }
        if !matches!(discussion.record.state.as_str(), "open" | "closed") {
            report.errors.push(format!(
                "{}: state must be open or closed",
                record_path.display()
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn validator_accepts_discussion() {
        let root = test_root("valid");
        write_discussion(
            &root,
            "discussion-0001",
            "discussion-0001",
            "General",
            "open",
        );

        let report = crate::validate(&root);
        assert!(report.is_ok(), "{:?}", report.errors);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validator_rejects_discussion_identity_mismatch() {
        let root = test_root("mismatch");
        write_discussion(
            &root,
            "discussion-0001",
            "discussion-9999",
            "General",
            "open",
        );

        let report = crate::validate(&root);
        assert!(!report.is_ok());
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("must match directory"))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validator_rejects_empty_title_and_invalid_state() {
        let root = test_root("invalid");
        write_discussion(
            &root,
            "discussion-0001",
            "discussion-0001",
            "   ",
            "deleted",
        );

        let report = crate::validate(&root);
        assert!(!report.is_ok());
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("title must not be empty"))
        );
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("state must be open or closed"))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn loader_requires_body_file() {
        let root = test_root("missing-body");
        let directory = root.join(".project/discussions/discussion-0001");
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("discussion.toml"),
            "schema = \"allodium.discussion/v0\"\nid = \"discussion-0001\"\ntitle = \"General\"\nstate = \"open\"\n",
        )
        .unwrap();

        let error = load_discussions(&root).unwrap_err();
        assert!(error.contains("body.md"));
        fs::remove_dir_all(root).unwrap();
    }

    fn write_discussion(root: &Path, directory_name: &str, id: &str, title: &str, state: &str) {
        let directory = root.join(".project/discussions").join(directory_name);
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("discussion.toml"),
            format!(
                "schema = \"allodium.discussion/v0\"\nid = \"{id}\"\ntitle = \"{title}\"\nstate = \"{state}\"\n"
            ),
        )
        .unwrap();
        fs::write(directory.join("body.md"), "Discussion body\n").unwrap();
    }

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-discussion-{name}-{}-{nonce}",
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
