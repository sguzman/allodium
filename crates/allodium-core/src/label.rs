use crate::ValidationReport;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const LABEL_SCHEMA_V0: &str = "allodium.label/v0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LabelRecord {
    pub schema: String,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalLabel {
    pub record: LabelRecord,
    pub path: PathBuf,
}

pub fn load_labels(root: impl AsRef<Path>) -> Result<Vec<CanonicalLabel>, String> {
    let labels_dir = root.as_ref().join(".project/labels");
    if !labels_dir.exists() {
        return Ok(Vec::new());
    }
    if !labels_dir.is_dir() {
        return Err(format!("{}: expected a directory", labels_dir.display()));
    }

    let mut entries = fs::read_dir(&labels_dir)
        .map_err(|error| format!("{}: {error}", labels_dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("{}: {error}", labels_dir.display()))?;
    entries.sort_by_key(|entry| entry.file_name());

    let mut labels = Vec::new();
    for entry in entries {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("toml") {
            continue;
        }
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let record: LabelRecord =
            toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
        labels.push(CanonicalLabel { record, path });
    }
    labels.sort_by(|left, right| left.record.id.cmp(&right.record.id));
    Ok(labels)
}

pub(crate) fn validate_labels(root: &Path, report: &mut ValidationReport) {
    let labels = match load_labels(root) {
        Ok(labels) => labels,
        Err(error) => {
            report.errors.push(error);
            return;
        }
    };

    for label in labels {
        let expected_id = label
            .path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default();

        if label.record.schema != LABEL_SCHEMA_V0 {
            report.errors.push(format!(
                "{}: unsupported schema {:?}; expected {:?}",
                label.path.display(),
                label.record.schema,
                LABEL_SCHEMA_V0
            ));
        }
        if label.record.id != expected_id {
            report.errors.push(format!(
                "{}: id {:?} must match filename stem {:?}",
                label.path.display(),
                label.record.id,
                expected_id
            ));
        }
        if label.record.name.trim().is_empty() {
            report
                .errors
                .push(format!("{}: name must not be empty", label.path.display()));
        }
        if let Some(color) = &label.record.color {
            if !valid_color(color) {
                report.errors.push(format!(
                    "{}: color must be #RRGGBB",
                    label.path.display()
                ));
            }
        }
    }
}

fn valid_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..]
            .iter()
            .all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn validator_accepts_label() {
        let root = test_root("valid");
        write_label(
            &root,
            "label-allodium",
            "label-allodium",
            "allodium",
            Some("#6f42c1"),
        );

        let report = crate::validate(&root);
        assert!(report.is_ok(), "{:?}", report.errors);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validator_rejects_label_identity_mismatch() {
        let root = test_root("mismatch");
        write_label(
            &root,
            "label-allodium",
            "label-other",
            "allodium",
            Some("#6f42c1"),
        );

        let report = crate::validate(&root);
        assert!(!report.is_ok());
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("must match filename stem"))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validator_rejects_empty_name_and_invalid_color() {
        let root = test_root("invalid");
        write_label(
            &root,
            "label-allodium",
            "label-allodium",
            "   ",
            Some("6f42c1"),
        );

        let report = crate::validate(&root);
        assert!(!report.is_ok());
        assert!(report.errors.iter().any(|error| error.contains("name must not be empty")));
        assert!(report.errors.iter().any(|error| error.contains("color must be #RRGGBB")));
        fs::remove_dir_all(root).unwrap();
    }

    fn write_label(root: &Path, filename: &str, id: &str, name: &str, color: Option<&str>) {
        let directory = root.join(".project/labels");
        fs::create_dir_all(&directory).unwrap();
        let color = color
            .map(|color| format!("color = \"{color}\"\n"))
            .unwrap_or_default();
        fs::write(
            directory.join(format!("{filename}.toml")),
            format!(
                "schema = \"allodium.label/v0\"\nid = \"{id}\"\nname = \"{name}\"\ndescription = \"Test label\"\n{color}"
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
            "allodium-label-{name}-{}-{nonce}",
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
