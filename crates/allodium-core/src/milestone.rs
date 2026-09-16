use crate::ValidationReport;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const MILESTONE_SCHEMA_V0: &str = "allodium.milestone/v0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MilestoneRecord {
    pub schema: String,
    pub id: String,
    pub title: String,
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalMilestone {
    pub record: MilestoneRecord,
    pub description: String,
    pub directory: PathBuf,
}

pub fn load_milestones(root: impl AsRef<Path>) -> Result<Vec<CanonicalMilestone>, String> {
    let milestones_dir = root.as_ref().join(".project/milestones");
    if !milestones_dir.exists() {
        return Ok(Vec::new());
    }
    if !milestones_dir.is_dir() {
        return Err(format!(
            "{}: expected a directory",
            milestones_dir.display()
        ));
    }

    let mut entries = fs::read_dir(&milestones_dir)
        .map_err(|error| format!("{}: {error}", milestones_dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("{}: {error}", milestones_dir.display()))?;
    entries.sort_by_key(|entry| entry.file_name());

    let mut milestones = Vec::new();
    for entry in entries {
        let directory = entry.path();
        if !directory.is_dir() {
            continue;
        }
        let record_path = directory.join("milestone.toml");
        let description_path = directory.join("description.md");
        let text = fs::read_to_string(&record_path)
            .map_err(|error| format!("{}: {error}", record_path.display()))?;
        let record: MilestoneRecord =
            toml::from_str(&text).map_err(|error| format!("{}: {error}", record_path.display()))?;
        let description = fs::read_to_string(&description_path)
            .map_err(|error| format!("{}: {error}", description_path.display()))?;
        milestones.push(CanonicalMilestone {
            record,
            description,
            directory,
        });
    }
    milestones.sort_by(|left, right| left.record.id.cmp(&right.record.id));
    Ok(milestones)
}

pub(crate) fn validate_milestones(root: &Path, report: &mut ValidationReport) {
    let milestones = match load_milestones(root) {
        Ok(milestones) => milestones,
        Err(error) => {
            report.errors.push(error);
            return;
        }
    };

    for milestone in milestones {
        let record_path = milestone.directory.join("milestone.toml");
        let expected_id = milestone
            .directory
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();

        if milestone.record.schema != MILESTONE_SCHEMA_V0 {
            report.errors.push(format!(
                "{}: unsupported schema {:?}; expected {:?}",
                record_path.display(),
                milestone.record.schema,
                MILESTONE_SCHEMA_V0
            ));
        }
        if milestone.record.id != expected_id {
            report.errors.push(format!(
                "{}: id {:?} must match directory {:?}",
                record_path.display(),
                milestone.record.id,
                expected_id
            ));
        }
        if milestone.record.title.trim().is_empty() {
            report.errors.push(format!(
                "{}: title must not be empty",
                record_path.display()
            ));
        }
        if !matches!(milestone.record.state.as_str(), "open" | "closed") {
            report.errors.push(format!(
                "{}: state must be open or closed",
                record_path.display()
            ));
        }
        if let Some(due) = &milestone.record.due {
            if !valid_calendar_date(due) {
                report.errors.push(format!(
                    "{}: due must be an ISO 8601 calendar date (YYYY-MM-DD)",
                    record_path.display()
                ));
            }
        }
    }
}

fn valid_calendar_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if bytes
        .iter()
        .enumerate()
        .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return false;
    }

    let year = value[0..4].parse::<u32>().ok();
    let month = value[5..7].parse::<u32>().ok();
    let day = value[8..10].parse::<u32>().ok();
    let (Some(year), Some(month), Some(day)) = (year, month, day) else {
        return false;
    };
    if !(1..=12).contains(&month) || day == 0 {
        return false;
    }

    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    day <= max_day
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn validator_accepts_milestone_and_leap_day() {
        let root = test_root("valid");
        write_milestone(&root, "milestone-0001", "open", Some("2028-02-29"));

        let report = crate::validate(&root);
        assert!(report.is_ok(), "{:?}", report.errors);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validator_rejects_milestone_identity_mismatch() {
        let root = test_root("mismatch");
        write_milestone(&root, "milestone-0001", "open", None);
        fs::write(
            root.join(".project/milestones/milestone-0001/milestone.toml"),
            milestone_record("milestone-9999", "open", None),
        )
        .unwrap();

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
    fn validator_rejects_invalid_milestone_state_and_date() {
        let root = test_root("invalid");
        write_milestone(&root, "milestone-0001", "paused", Some("2026-02-29"));

        let report = crate::validate(&root);
        assert!(!report.is_ok());
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("state must be open or closed"))
        );
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("ISO 8601 calendar date"))
        );
        fs::remove_dir_all(root).unwrap();
    }

    fn write_milestone(root: &Path, id: &str, state: &str, due: Option<&str>) {
        let directory = root.join(".project/milestones").join(id);
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("milestone.toml"),
            milestone_record(id, state, due),
        )
        .unwrap();
        fs::write(directory.join("description.md"), "Milestone description\n").unwrap();
    }

    fn milestone_record(id: &str, state: &str, due: Option<&str>) -> String {
        let due = due
            .map(|due| format!("due = \"{due}\"\n"))
            .unwrap_or_default();
        format!(
            "schema = \"allodium.milestone/v0\"\nid = \"{id}\"\ntitle = \"Test milestone\"\nstate = \"{state}\"\n{due}"
        )
    }

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-milestone-{name}-{}-{nonce}",
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
