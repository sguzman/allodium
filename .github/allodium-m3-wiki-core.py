from pathlib import Path

core = Path('crates/allodium-core/src/lib.rs')
text = core.read_text()

old = 'pub mod github;\n'
new = 'pub mod github;\npub mod wiki;\n'
if old not in text:
    raise SystemExit('missing core module anchor')
text = text.replace(old, new, 1)

old = '    validate_remotes(root, &mut report);\n    report\n'
new = '    wiki::validate_wiki(root, &mut report);\n    validate_remotes(root, &mut report);\n    report\n'
if old not in text:
    raise SystemExit('missing validation anchor')
text = text.replace(old, new, 1)
core.write_text(text)

Path('crates/allodium-core/src/wiki.rs').write_text(r'''use crate::ValidationReport;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const WIKI_SCHEMA_V0: &str = "allodium.wiki/v0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WikiRecord {
    pub schema: String,
    pub title: String,
    pub home: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalWiki {
    pub record: WikiRecord,
    pub directory: PathBuf,
}

pub fn load_wiki(root: impl AsRef<Path>) -> Result<Option<CanonicalWiki>, String> {
    let directory = root.as_ref().join(".project/wiki");
    if !directory.exists() {
        return Ok(None);
    }
    if !directory.is_dir() {
        return Err(format!("{}: expected a directory", directory.display()));
    }

    let record_path = directory.join("wiki.toml");
    let text = fs::read_to_string(&record_path)
        .map_err(|error| format!("{}: {error}", record_path.display()))?;
    let record: WikiRecord =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", record_path.display()))?;

    Ok(Some(CanonicalWiki { record, directory }))
}

pub(crate) fn validate_wiki(root: &Path, report: &mut ValidationReport) {
    let Some(wiki) = (match load_wiki(root) {
        Ok(wiki) => wiki,
        Err(error) => {
            report.errors.push(error);
            return;
        }
    }) else {
        return;
    };

    let record_path = wiki.directory.join("wiki.toml");
    if wiki.record.schema != WIKI_SCHEMA_V0 {
        report.errors.push(format!(
            "{}: unsupported schema {:?}; expected {:?}",
            record_path.display(),
            wiki.record.schema,
            WIKI_SCHEMA_V0
        ));
    }
    if wiki.record.title.trim().is_empty() {
        report.errors.push(format!(
            "{}: title must not be empty",
            record_path.display()
        ));
    }

    match safe_relative_path(&wiki.record.home) {
        Ok(home) => {
            let home_path = wiki.directory.join(&home);
            match fs::symlink_metadata(&home_path) {
                Ok(metadata) if metadata.file_type().is_symlink() => report.errors.push(format!(
                    "{}: home page must be an ordinary file, not a symlink",
                    home_path.display()
                )),
                Ok(metadata) if !metadata.is_file() => report.errors.push(format!(
                    "{}: declared home page must be a file",
                    home_path.display()
                )),
                Ok(_) => {}
                Err(error) => report.errors.push(format!(
                    "{}: declared home page is not readable: {error}",
                    home_path.display()
                )),
            }
        }
        Err(error) => report
            .errors
            .push(format!("{}: {error}", record_path.display())),
    }
}

pub fn safe_relative_path(value: &str) -> Result<PathBuf, String> {
    if value.trim().is_empty() {
        return Err("home must not be empty".into());
    }

    let path = Path::new(value);
    if path.is_absolute() {
        return Err("home must be a relative path under .project/wiki".into());
    }

    let mut saw_component = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => saw_component = true,
            _ => {
                return Err(
                    "home must contain only normal relative path components; '.', '..', roots, and prefixes are forbidden"
                        .into(),
                );
            }
        }
    }
    if !saw_component {
        return Err("home must name a file under .project/wiki".into());
    }

    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn validator_accepts_wiki_metadata_and_home() {
        let root = test_root("valid");
        write_wiki(&root, "Home.md");
        fs::write(root.join(".project/wiki/Home.md"), "# Home\n").unwrap();

        let report = crate::validate(&root);
        assert!(report.is_ok(), "{:?}", report.errors);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validator_rejects_missing_wiki_metadata() {
        let root = test_root("missing-metadata");
        fs::create_dir_all(root.join(".project/wiki")).unwrap();
        fs::write(root.join(".project/wiki/Home.md"), "# Home\n").unwrap();

        let report = crate::validate(&root);
        assert!(!report.is_ok());
        assert!(report.errors.iter().any(|error| error.contains("wiki.toml")));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validator_rejects_home_escape() {
        let root = test_root("escape");
        write_wiki(&root, "../README.md");

        let report = crate::validate(&root);
        assert!(!report.is_ok());
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("normal relative path components"))
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validator_rejects_missing_home() {
        let root = test_root("missing-home");
        write_wiki(&root, "Home.md");

        let report = crate::validate(&root);
        assert!(!report.is_ok());
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("declared home page"))
        );

        fs::remove_dir_all(root).unwrap();
    }

    fn write_wiki(root: &Path, home: &str) {
        fs::create_dir_all(root.join(".project/wiki")).unwrap();
        fs::write(
            root.join(".project/wiki/wiki.toml"),
            format!(
                r#"schema = "allodium.wiki/v0"
title = "Test Wiki"
home = "{home}"
"#
            ),
        )
        .unwrap();
    }

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-wiki-{name}-{}-{nonce}",
            std::process::id()
        ));

        fs::create_dir_all(root.join(".project/issues/issue-0001")).unwrap();
        fs::create_dir_all(root.join(".project/remotes/github")).unwrap();
        fs::write(
            root.join(".project/manifest.toml"),
            r#"schema = "allodium.project/v0"
id = "test"
name = "Test"
"#,
        )
        .unwrap();
        fs::write(
            root.join(".project/issues/issue-0001/issue.toml"),
            r#"schema = "allodium.issue/v0"
id = "issue-0001"
title = "Test"
state = "open"
"#,
        )
        .unwrap();
        fs::write(root.join(".project/issues/issue-0001/body.md"), "Body\n").unwrap();
        fs::write(
            root.join(".project/remotes/github/remote.toml"),
            r#"schema = "allodium.remote/v0"
name = "github"
kind = "github"
repository = "owner/repo"
outbound = "reconcile"
inbound = "archive"
"#,
        )
        .unwrap();
        root
    }
}
''')
