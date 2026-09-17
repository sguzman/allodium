use crate::github::{GitHubOperation, GitHubPlan, PLAN_SCHEMA_V0};
use crate::label::{CanonicalLabel, load_labels};
use crate::load_remote;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const LABEL_MAPPINGS_SCHEMA_V0: &str = "allodium.github.label-mappings/v0";
pub const OBSERVED_LABEL_SCHEMA_V0: &str = "allodium.github.observed-label/v0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LabelMapping {
    pub remote_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LabelMappings {
    pub schema: String,
    #[serde(default)]
    pub labels: BTreeMap<String, LabelMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedLabel {
    pub schema: String,
    pub canonical_id: String,
    pub remote_id: u64,
    pub node_id: String,
    pub url: String,
    pub name: String,
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub default: bool,
    pub observed_at: String,
}

pub fn plan_labels(root: impl AsRef<Path>, remote_name: &str) -> Result<GitHubPlan, String> {
    let root = root.as_ref();
    let remote = load_remote(root, remote_name)?;
    if remote.kind != "github" {
        return Err(format!(
            "remote {remote_name:?} has kind {:?}, not \"github\"",
            remote.kind
        ));
    }

    let mappings = load_label_mappings(root, remote_name)?;
    let labels = load_labels(root)?;
    let mut operations = Vec::new();
    for label in labels {
        plan_label(root, remote_name, &label, &mappings, &mut operations)?;
    }

    Ok(GitHubPlan {
        schema: PLAN_SCHEMA_V0.into(),
        remote: remote_name.into(),
        repository: remote.repository,
        operations,
    })
}

pub fn load_label_mappings(
    root: impl AsRef<Path>,
    remote_name: &str,
) -> Result<LabelMappings, String> {
    let path = label_mappings_path(root.as_ref(), remote_name);
    if !path.exists() {
        return Ok(LabelMappings {
            schema: LABEL_MAPPINGS_SCHEMA_V0.into(),
            labels: BTreeMap::new(),
        });
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mappings: LabelMappings =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if mappings.schema != LABEL_MAPPINGS_SCHEMA_V0 {
        return Err(format!(
            "{}: unsupported label mapping schema {:?}; expected {:?}",
            path.display(),
            mappings.schema,
            LABEL_MAPPINGS_SCHEMA_V0
        ));
    }
    Ok(mappings)
}

pub fn save_label_mappings(
    root: impl AsRef<Path>,
    remote_name: &str,
    mappings: &LabelMappings,
) -> Result<(), String> {
    if mappings.schema != LABEL_MAPPINGS_SCHEMA_V0 {
        return Err(format!(
            "refusing to write unsupported label mapping schema {:?}",
            mappings.schema
        ));
    }
    let path = label_mappings_path(root.as_ref(), remote_name);
    fs::create_dir_all(path.parent().expect("label mapping path has parent"))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let text = toml::to_string_pretty(mappings)
        .map_err(|error| format!("could not serialize GitHub label mappings: {error}"))?;
    fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn load_observed_label(
    root: impl AsRef<Path>,
    remote_name: &str,
    canonical_id: &str,
) -> Result<Option<ObservedLabel>, String> {
    let path = observed_label_path(root.as_ref(), remote_name, canonical_id);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let observed: ObservedLabel =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if observed.schema != OBSERVED_LABEL_SCHEMA_V0 {
        return Err(format!(
            "{}: unsupported observed label schema {:?}; expected {:?}",
            path.display(),
            observed.schema,
            OBSERVED_LABEL_SCHEMA_V0
        ));
    }
    Ok(Some(observed))
}

pub fn write_observed_label_snapshot(
    root: impl AsRef<Path>,
    remote_name: &str,
    observed: &ObservedLabel,
) -> Result<(), String> {
    if observed.schema != OBSERVED_LABEL_SCHEMA_V0 {
        return Err(format!(
            "refusing to write unsupported observed label schema {:?}",
            observed.schema
        ));
    }
    let path = observed_label_path(root.as_ref(), remote_name, &observed.canonical_id);
    fs::create_dir_all(path.parent().expect("observed label path has parent"))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let text = toml::to_string_pretty(observed)
        .map_err(|error| format!("could not serialize observed GitHub label: {error}"))?;
    fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))
}

fn plan_label(
    root: &Path,
    remote_name: &str,
    label: &CanonicalLabel,
    mappings: &LabelMappings,
    operations: &mut Vec<GitHubOperation>,
) -> Result<(), String> {
    let canonical_id = &label.record.id;
    let Some(mapping) = mappings.labels.get(canonical_id) else {
        operations.push(GitHubOperation {
            canonical_id: canonical_id.clone(),
            action: "create_label".into(),
            number: None,
            fields: managed_fields(label),
            reason: "canonical label has no GitHub label mapping".into(),
        });
        return Ok(());
    };

    let Some(observed) = load_observed_label(root, remote_name, canonical_id)? else {
        operations.push(GitHubOperation {
            canonical_id: canonical_id.clone(),
            action: "observe_label".into(),
            number: Some(mapping.remote_id),
            fields: managed_fields(label),
            reason: "label mapping exists but the local observed GitHub label snapshot is missing"
                .into(),
        });
        return Ok(());
    };

    if observed.canonical_id != *canonical_id || observed.remote_id != mapping.remote_id {
        return Err(format!(
            "observed GitHub label snapshot for {canonical_id:?} disagrees with its stable provider-ID mapping"
        ));
    }

    let mut fields = Vec::new();
    if observed.name != label.record.name {
        fields.push("name".into());
    }
    if label.record.description.is_some() && observed.description != label.record.description {
        fields.push("description".into());
    }
    if let Some(color) = canonical_provider_color(label) {
        if !observed.color.eq_ignore_ascii_case(&color) {
            fields.push("color".into());
        }
    }
    if !fields.is_empty() {
        operations.push(GitHubOperation {
            canonical_id: canonical_id.clone(),
            action: "update_label".into(),
            number: Some(mapping.remote_id),
            fields,
            reason:
                "canonical managed label fields differ from the last observed GitHub label state"
                    .into(),
        });
    }
    Ok(())
}

fn canonical_provider_color(label: &CanonicalLabel) -> Option<String> {
    label.record.color.as_deref().map(|color| {
        color
            .strip_prefix('#')
            .unwrap_or(color)
            .to_ascii_lowercase()
    })
}

fn managed_fields(label: &CanonicalLabel) -> Vec<String> {
    let mut fields = vec!["name".into()];
    if label.record.description.is_some() {
        fields.push("description".into());
    }
    if label.record.color.is_some() {
        fields.push("color".into());
    }
    fields
}

fn label_mappings_path(root: &Path, remote_name: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("mappings/labels.toml")
}

fn observed_label_path(root: &Path, remote_name: &str, canonical_id: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("observed/labels")
        .join(format!("{canonical_id}.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn unmapped_label_plans_create() {
        let root = test_project("create", true);
        let plan = plan_labels(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "create_label");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mapped_label_without_observation_plans_observe_by_provider_id() {
        let root = test_project("observe", true);
        save_mapping(&root);
        let plan = plan_labels(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "observe_label");
        assert_eq!(plan.operations[0].number, Some(4242));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn matching_label_is_idempotent() {
        let root = test_project("idempotent", true);
        save_mapping(&root);
        save_observation(&root, "allodium", "6f42c1", Some("Test label"));
        let plan = plan_labels(&root, "github").unwrap();
        assert!(plan.operations.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn provider_rename_and_mutable_drift_plan_update_on_same_provider_id() {
        let root = test_project("rename", true);
        save_mapping(&root);
        save_observation(&root, "provider-renamed", "ffffff", None);
        let plan = plan_labels(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "update_label");
        assert_eq!(plan.operations[0].number, Some(4242));
        for field in ["name", "description", "color"] {
            assert!(plan.operations[0].fields.contains(&field.to_string()));
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn absent_optional_fields_are_unmanaged() {
        let root = test_project("optional-unmanaged", true);
        fs::write(
            root.join(".project/labels/label-allodium.toml"),
            "schema = \"allodium.label/v0\"\nid = \"label-allodium\"\nname = \"allodium\"\n",
        )
        .unwrap();
        save_mapping(&root);
        save_observation(
            &root,
            "allodium",
            "ffffff",
            Some("Provider-owned description"),
        );
        let plan = plan_labels(&root, "github").unwrap();
        assert!(plan.operations.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn observation_with_different_provider_id_is_rejected() {
        let root = test_project("identity", true);
        save_mapping(&root);
        let mut observed = observation("allodium", "6f42c1", Some("Test label"));
        observed.remote_id = 9999;
        write_observed_label_snapshot(&root, "github", &observed).unwrap();
        let error = plan_labels(&root, "github").unwrap_err();
        assert!(error.contains("stable provider-ID mapping"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn absent_canonical_label_does_not_plan_delete() {
        let root = test_project("no-delete", false);
        save_mapping(&root);
        let plan = plan_labels(&root, "github").unwrap();
        assert!(plan.operations.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    fn test_project(name: &str, with_label: bool) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-github-label-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join(".project/remotes/github")).unwrap();
        fs::write(
            root.join(".project/remotes/github/remote.toml"),
            "schema = \"allodium.remote/v0\"\nname = \"github\"\nkind = \"github\"\nrepository = \"owner/repo\"\noutbound = \"reconcile\"\ninbound = \"archive\"\n",
        )
        .unwrap();
        if with_label {
            fs::create_dir_all(root.join(".project/labels")).unwrap();
            fs::write(
                root.join(".project/labels/label-allodium.toml"),
                "schema = \"allodium.label/v0\"\nid = \"label-allodium\"\nname = \"allodium\"\ndescription = \"Test label\"\ncolor = \"#6f42c1\"\n",
            )
            .unwrap();
        }
        root
    }

    fn save_mapping(root: &Path) {
        let mut mappings = LabelMappings {
            schema: LABEL_MAPPINGS_SCHEMA_V0.into(),
            labels: BTreeMap::new(),
        };
        mappings
            .labels
            .insert("label-allodium".into(), LabelMapping { remote_id: 4242 });
        save_label_mappings(root, "github", &mappings).unwrap();
    }

    fn save_observation(root: &Path, name: &str, color: &str, description: Option<&str>) {
        let observed = observation(name, color, description);
        write_observed_label_snapshot(root, "github", &observed).unwrap();
    }

    fn observation(name: &str, color: &str, description: Option<&str>) -> ObservedLabel {
        ObservedLabel {
            schema: OBSERVED_LABEL_SCHEMA_V0.into(),
            canonical_id: "label-allodium".into(),
            remote_id: 4242,
            node_id: "LA_kwDOExample".into(),
            url: format!("https://api.github.com/repos/owner/repo/labels/{name}"),
            name: name.into(),
            color: color.into(),
            description: description.map(str::to_owned),
            default: false,
            observed_at: "2026-09-17T00:00:00Z".into(),
        }
    }
}
