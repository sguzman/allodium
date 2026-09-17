use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const OBSERVED_PROJECT_PROVIDER_STATE_SCHEMA_V0: &str =
    "allodium.github.observed-project-provider-state/v0";
pub const PROJECT_PROVIDER_CHANGE_SCHEMA_V0: &str = "allodium.github.project-provider-change/v0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedProviderOption {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedProviderIteration {
    pub id: String,
    pub title: String,
    pub start_date: String,
    pub duration: i64,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedProviderField {
    pub node_id: String,
    pub provider_type: String,
    pub name: String,
    pub data_type: String,
    pub remote_updated_at: String,
    #[serde(default)]
    pub options: Vec<ObservedProviderOption>,
    #[serde(default)]
    pub iterations: Vec<ObservedProviderIteration>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedProviderContent {
    pub provider_type: String,
    pub node_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedProviderFieldValue {
    pub provider_type: String,
    pub field_node_id: String,
    pub field_name: String,
    pub value: String,
    pub remote_updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedProviderItem {
    pub node_id: String,
    pub item_type: String,
    pub remote_updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<ObservedProviderContent>,
    #[serde(default)]
    pub values: Vec<ObservedProviderFieldValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedProviderView {
    pub node_id: String,
    pub number: u64,
    pub name: String,
    pub layout: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedProjectProviderState {
    pub schema: String,
    pub canonical_id: String,
    pub number: u64,
    pub node_id: String,
    pub url: String,
    pub owner_node_id: String,
    pub title: String,
    #[serde(default)]
    pub short_description: String,
    pub closed: bool,
    pub remote_updated_at: String,
    pub observed_at: String,
    #[serde(default)]
    pub fields: Vec<ObservedProviderField>,
    #[serde(default)]
    pub items: Vec<ObservedProviderItem>,
    #[serde(default)]
    pub views: Vec<ObservedProviderView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ProjectProviderChangeEvent {
    schema: String,
    id: String,
    remote: String,
    kind: String,
    canonical_id: String,
    project_node_id: String,
    observed_at: String,
    evidence: String,
}

pub fn load_observed_project_provider_state(
    root: impl AsRef<Path>,
    remote_name: &str,
    canonical_id: &str,
) -> Result<Option<ObservedProjectProviderState>, String> {
    let path = observed_state_path(root.as_ref(), remote_name, canonical_id);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let observed: ObservedProjectProviderState =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if observed.schema != OBSERVED_PROJECT_PROVIDER_STATE_SCHEMA_V0
        || observed.canonical_id != canonical_id
    {
        return Err(format!(
            "{}: observed Project provider-state schema/canonical identity mismatch",
            path.display()
        ));
    }
    Ok(Some(observed))
}

pub fn write_observed_project_provider_state(
    root: impl AsRef<Path>,
    remote_name: &str,
    observed: &ObservedProjectProviderState,
) -> Result<(), String> {
    if observed.schema != OBSERVED_PROJECT_PROVIDER_STATE_SCHEMA_V0 {
        return Err(format!(
            "refusing to write unsupported observed Project provider-state schema {:?}",
            observed.schema
        ));
    }
    let path = observed_state_path(root.as_ref(), remote_name, &observed.canonical_id);
    fs::create_dir_all(
        path.parent()
            .expect("observed Project state path has parent"),
    )
    .map_err(|error| format!("{}: {error}", path.display()))?;
    let text = toml::to_string_pretty(observed)
        .map_err(|error| format!("could not serialize observed Project provider state: {error}"))?;
    fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn archive_project_provider_change(
    root: impl AsRef<Path>,
    remote_name: &str,
    previous: &ObservedProjectProviderState,
    current: &ObservedProjectProviderState,
) -> Result<bool, String> {
    if same_provider_state(previous, current) {
        return Ok(false);
    }
    if previous.canonical_id != current.canonical_id || previous.node_id != current.node_id {
        return Err(
            "refusing to archive Project provider change across different identities".into(),
        );
    }
    let day = current.observed_at.get(0..10).unwrap_or("undated");
    let stamp = current
        .observed_at
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() {
                value
            } else {
                '-'
            }
        })
        .collect::<String>();
    let id = format!("{remote_name}-project-{}-{stamp}", current.canonical_id);
    let directory = root
        .as_ref()
        .join(".project/remotes")
        .join(remote_name)
        .join("incoming")
        .join(day)
        .join(&id);
    if directory.exists() {
        return Ok(false);
    }
    fs::create_dir_all(&directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    let event = ProjectProviderChangeEvent {
        schema: PROJECT_PROVIDER_CHANGE_SCHEMA_V0.into(),
        id,
        remote: remote_name.into(),
        kind: "project.provider_state.changed".into(),
        canonical_id: current.canonical_id.clone(),
        project_node_id: current.node_id.clone(),
        observed_at: current.observed_at.clone(),
        evidence: "GitHub ProjectV2 provider state changed relative to the prior persisted observation; evidence is archived without canonical promotion".into(),
    };
    let event_text = toml::to_string_pretty(&event)
        .map_err(|error| format!("could not serialize Project provider change event: {error}"))?;
    let previous_text = toml::to_string_pretty(previous)
        .map_err(|error| format!("could not serialize previous Project provider state: {error}"))?;
    let current_text = toml::to_string_pretty(current)
        .map_err(|error| format!("could not serialize current Project provider state: {error}"))?;
    fs::write(directory.join("event.toml"), event_text)
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    fs::write(directory.join("previous.toml"), previous_text)
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    fs::write(directory.join("current.toml"), current_text)
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    Ok(true)
}

fn same_provider_state(
    left: &ObservedProjectProviderState,
    right: &ObservedProjectProviderState,
) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    left.observed_at.clear();
    right.observed_at.clear();
    left == right
}

fn observed_state_path(root: &Path, remote_name: &str, canonical_id: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("observed/projects/state")
        .join(format!("{canonical_id}.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn identical_reobservation_does_not_archive_noise() {
        let root = test_root("same");
        let previous = snapshot("2026-09-17T20:00:00Z", "Todo");
        let current = snapshot("2026-09-17T20:01:00Z", "Todo");
        assert!(!archive_project_provider_change(&root, "github", &previous, &current).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn provider_change_archives_both_sides_without_canonical_promotion() {
        let root = test_root("changed");
        let previous = snapshot("2026-09-17T20:00:00Z", "Todo");
        let current = snapshot("2026-09-17T20:01:00Z", "Doing");
        assert!(archive_project_provider_change(&root, "github", &previous, &current).unwrap());
        let incoming = root.join(".project/remotes/github/incoming/2026-09-17");
        let entries = fs::read_dir(incoming).unwrap().count();
        assert_eq!(entries, 1);
        assert!(!root.join(".project/issues/provider-draft").exists());
        fs::remove_dir_all(root).unwrap();
    }

    fn snapshot(observed_at: &str, option_name: &str) -> ObservedProjectProviderState {
        ObservedProjectProviderState {
            schema: OBSERVED_PROJECT_PROVIDER_STATE_SCHEMA_V0.into(),
            canonical_id: "board-0001".into(),
            number: 7,
            node_id: "PVT_project".into(),
            url: "https://github.com/users/sguzman/projects/7".into(),
            owner_node_id: "U_owner".into(),
            title: "Board".into(),
            short_description: String::new(),
            closed: false,
            remote_updated_at: "2026-09-17T19:59:00Z".into(),
            observed_at: observed_at.into(),
            fields: vec![ObservedProviderField {
                node_id: "PVTSSF_status".into(),
                provider_type: "ProjectV2SingleSelectField".into(),
                name: "Status".into(),
                data_type: "SINGLE_SELECT".into(),
                remote_updated_at: "2026-09-17T19:59:00Z".into(),
                options: vec![ObservedProviderOption {
                    id: "option-1".into(),
                    name: option_name.into(),
                }],
                iterations: Vec::new(),
            }],
            items: Vec::new(),
            views: Vec::new(),
        }
    }

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-project-provider-observation-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }
}
