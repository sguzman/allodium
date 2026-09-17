from pathlib import Path

CORE_OBSERVATION = r'''use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const OBSERVED_PROJECT_PROVIDER_STATE_SCHEMA_V0: &str =
    "allodium.github.observed-project-provider-state/v0";
pub const PROJECT_PROVIDER_CHANGE_SCHEMA_V0: &str =
    "allodium.github.project-provider-change/v0";

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
    fs::create_dir_all(path.parent().expect("observed Project state path has parent"))
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
        return Err("refusing to archive Project provider change across different identities".into());
    }
    let day = current.observed_at.get(0..10).unwrap_or("undated");
    let stamp = current
        .observed_at
        .chars()
        .map(|value| if value.is_ascii_alphanumeric() { value } else { '-' })
        .collect::<String>();
    let id = format!(
        "{remote_name}-project-{}-{stamp}",
        current.canonical_id
    );
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
    fs::create_dir_all(&directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?;
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
'''

PROJECT_RUNTIME = r'''use super::{GitHubAdapter, now};
use allodium_core::board::{CanonicalBoard, load_boards};
use allodium_core::github_project::{
    OBSERVED_PROJECTS_CAPABILITIES_SCHEMA_V0, OBSERVED_PROJECT_CONTENT_SCHEMA_V0,
    OBSERVED_PROJECT_SCHEMA_V0, ObservedProject, ObservedProjectContentIdentity,
    ObservedProjectsCapabilities, ProjectItemMapping, ProjectMapping, ProjectMappings,
    ProjectsProjectionConfig, load_observed_project_content, load_project_mappings,
    load_projects_projection_config, save_project_mappings, write_observed_project,
    write_observed_project_content, write_observed_projects_capabilities,
};
use allodium_core::github_project_observation::{
    OBSERVED_PROJECT_PROVIDER_STATE_SCHEMA_V0, ObservedProjectProviderState,
    ObservedProviderContent, ObservedProviderField, ObservedProviderFieldValue,
    ObservedProviderItem, ObservedProviderIteration, ObservedProviderOption, ObservedProviderView,
    archive_project_provider_change, load_observed_project_provider_state,
    write_observed_project_provider_state,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ProjectsObserveReport {
    pub capabilities_observed: usize,
    pub projects_observed: usize,
    pub provider_changes_archived: usize,
    pub item_identities_mapped: usize,
}

pub(super) fn observe_repository_content_identity(
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    remote_type: &str,
    number: u64,
    node_id: &str,
    observed_at: &str,
) -> Result<bool, String> {
    if node_id.trim().is_empty() || !projected_object(root, remote_name, canonical_id)? {
        return Ok(false);
    }
    write_observed_project_content(
        root,
        remote_name,
        &ObservedProjectContentIdentity {
            schema: OBSERVED_PROJECT_CONTENT_SCHEMA_V0.into(),
            canonical_id: canonical_id.into(),
            remote_type: remote_type.into(),
            number,
            node_id: node_id.into(),
            observed_at: observed_at.into(),
        },
    )?;
    Ok(true)
}

pub(super) fn observe_projects(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
) -> Result<ProjectsObserveReport, String> {
    let boards = load_boards(root)?;
    if boards.is_empty() {
        return Ok(ProjectsObserveReport::default());
    }
    let Some(config) = load_projects_projection_config(root, remote_name)? else {
        return Ok(ProjectsObserveReport::default());
    };
    let enabled = boards
        .iter()
        .filter(|board| {
            config
                .boards
                .get(&board.record.id)
                .is_some_and(|binding| binding.enabled)
        })
        .collect::<Vec<_>>();
    if enabled.is_empty() {
        return Ok(ProjectsObserveReport::default());
    }

    let token = env::var(&config.credential_env)
        .ok()
        .filter(|value| !value.trim().is_empty());
    let (authenticated, owner_node_id) = match token.as_deref() {
        None => (false, None),
        Some(token) => match probe_projects_owner(adapter, &config, token)? {
            Some(owner_node_id) => (true, Some(owner_node_id)),
            None => (false, None),
        },
    };

    let observed_at = now();
    let observed = ObservedProjectsCapabilities {
        schema: OBSERVED_PROJECTS_CAPABILITIES_SCHEMA_V0.into(),
        credential_available: token.is_some(),
        authenticated,
        owner_kind: config.owner_kind.clone(),
        owner: config.owner.clone(),
        owner_node_id: owner_node_id.clone(),
        observed_at: observed_at.clone(),
    };
    write_observed_projects_capabilities(root, remote_name, &observed)?;
    let mut report = ProjectsObserveReport {
        capabilities_observed: 1,
        ..ProjectsObserveReport::default()
    };
    if !authenticated {
        return Ok(report);
    }
    let token = token.as_deref().expect("authenticated probe had credential");
    let owner_node_id = owner_node_id
        .as_deref()
        .expect("authenticated probe had owner node ID");
    let mut mappings = load_project_mappings(root, remote_name)?;
    let mut mappings_changed = false;

    for board in enabled {
        let binding = config
            .boards
            .get(&board.record.id)
            .expect("enabled board had binding");
        let existing_mapping = mappings.boards.get(&board.record.id).cloned();
        let provider = match existing_mapping.as_ref() {
            Some(mapping) => fetch_project_by_node(adapter, token, &mapping.node_id)?,
            None if binding.target == "existing" => fetch_project_by_number(
                adapter,
                &config,
                token,
                binding.project_number.expect("validated existing target has number"),
            )?,
            None => continue,
        };
        let provider = provider.ok_or_else(|| {
            format!(
                "configured GitHub ProjectV2 target for {:?} was not found; refusing title-based fallback",
                board.record.id
            )
        })?;
        if provider.owner_node_id != owner_node_id {
            return Err(format!(
                "GitHub ProjectV2 {:?} is owned by {:?}, not authenticated configured owner {:?}",
                provider.node_id, provider.owner_node_id, owner_node_id
            ));
        }

        let mapping = mappings
            .boards
            .entry(board.record.id.clone())
            .or_insert_with(|| ProjectMapping {
                number: provider.number,
                node_id: provider.node_id.clone(),
                url: provider.url.clone(),
                owner_node_id: provider.owner_node_id.clone(),
                items: BTreeMap::new(),
                fields: BTreeMap::new(),
                views: BTreeMap::new(),
            });
        if mapping.number != provider.number
            || mapping.node_id != provider.node_id
            || mapping.owner_node_id != provider.owner_node_id
        {
            return Err(format!(
                "persisted GitHub ProjectV2 mapping for {:?} disagrees with observed provider identity",
                board.record.id
            ));
        }
        if existing_mapping.is_none() {
            mappings_changed = true;
        }

        let snapshot = provider.into_snapshot(&board.record.id, &observed_at)?;
        if let Some(previous) =
            load_observed_project_provider_state(root, remote_name, &board.record.id)?
        {
            if archive_project_provider_change(root, remote_name, &previous, &snapshot)? {
                report.provider_changes_archived += 1;
            }
        }
        write_observed_project_provider_state(root, remote_name, &snapshot)?;
        write_observed_project(
            root,
            remote_name,
            &ObservedProject {
                schema: OBSERVED_PROJECT_SCHEMA_V0.into(),
                canonical_id: board.record.id.clone(),
                number: snapshot.number,
                node_id: snapshot.node_id.clone(),
                url: snapshot.url.clone(),
                owner_node_id: snapshot.owner_node_id.clone(),
                title: snapshot.title.clone(),
                short_description: snapshot.short_description.clone(),
                closed: snapshot.closed,
                remote_updated_at: snapshot.remote_updated_at.clone(),
                observed_at: snapshot.observed_at.clone(),
            },
        )?;
        report.projects_observed += 1;

        for board_item in &board.items {
            let canonical_id = &board_item.record.object;
            let Some(content_identity) =
                load_observed_project_content(root, remote_name, canonical_id)?
            else {
                continue;
            };
            let Some(provider_item) = snapshot.items.iter().find(|item| {
                item.content
                    .as_ref()
                    .is_some_and(|content| content.node_id == content_identity.node_id)
            }) else {
                continue;
            };
            let new_mapping = ProjectItemMapping {
                node_id: provider_item.node_id.clone(),
                content_node_id: content_identity.node_id.clone(),
            };
            match mapping.items.get(canonical_id) {
                None => {
                    mapping.items.insert(canonical_id.clone(), new_mapping);
                    mappings_changed = true;
                    report.item_identities_mapped += 1;
                }
                Some(existing) if existing == &new_mapping => {}
                Some(_) => {
                    return Err(format!(
                        "GitHub ProjectV2 item identity for {canonical_id:?} disagrees with persisted mapping; refusing automatic remap"
                    ));
                }
            }
        }
    }

    if mappings_changed {
        save_project_mappings(root, remote_name, &mappings)?;
    }
    Ok(report)
}

fn projected_object(root: &Path, remote_name: &str, canonical_id: &str) -> Result<bool, String> {
    let Some(config) = load_projects_projection_config(root, remote_name)? else {
        return Ok(false);
    };
    for board in load_boards(root)? {
        if !config
            .boards
            .get(&board.record.id)
            .is_some_and(|binding| binding.enabled)
        {
            continue;
        }
        if board
            .items
            .iter()
            .any(|item| item.record.object == canonical_id)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn probe_projects_owner(
    adapter: &GitHubAdapter,
    config: &ProjectsProjectionConfig,
    token: &str,
) -> Result<Option<String>, String> {
    let (owner_field, query) = match config.owner_kind.as_str() {
        "user" => (
            "user",
            "query($login: String!) { user(login: $login) { id projectsV2(first: 1) { totalCount } } }",
        ),
        "organization" => (
            "organization",
            "query($login: String!) { organization(login: $login) { id projectsV2(first: 1) { totalCount } } }",
        ),
        other => return Err(format!("unsupported GitHub Projects owner kind {other:?}")),
    };
    let payload = graphql(adapter, token, query, json!({ "login": config.owner }))?;
    if payload
        .get("errors")
        .and_then(Value::as_array)
        .is_some_and(|errors| !errors.is_empty())
    {
        return Ok(None);
    }
    Ok(payload
        .get("data")
        .and_then(|data| data.get(owner_field))
        .and_then(|owner| owner.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned))
}

fn fetch_project_by_number(
    adapter: &GitHubAdapter,
    config: &ProjectsProjectionConfig,
    token: &str,
    number: u64,
) -> Result<Option<ProviderProject>, String> {
    let owner_field = config.owner_kind.as_str();
    let query = format!(
        "query($login: String!, $number: Int!) {{ {owner_field}(login: $login) {{ projectV2(number: $number) {{ {} }} }} }}",
        project_selection()
    );
    let payload = graphql(
        adapter,
        token,
        &query,
        json!({ "login": config.owner, "number": number }),
    )?;
    graphql_errors(&payload)?;
    let project = payload
        .get("data")
        .and_then(|data| data.get(owner_field))
        .and_then(|owner| owner.get("projectV2"));
    parse_optional_project(project)
}

fn fetch_project_by_node(
    adapter: &GitHubAdapter,
    token: &str,
    node_id: &str,
) -> Result<Option<ProviderProject>, String> {
    let query = format!(
        "query($id: ID!) {{ node(id: $id) {{ ... on ProjectV2 {{ {} }} }} }}",
        project_selection()
    );
    let payload = graphql(adapter, token, &query, json!({ "id": node_id }))?;
    graphql_errors(&payload)?;
    parse_optional_project(payload.get("data").and_then(|data| data.get("node")))
}

fn project_selection() -> &'static str {
    r#"
        id number url title shortDescription closed updatedAt owner { id }
        fields(first: 100) {
          nodes {
            __typename
            ... on ProjectV2Field { id name dataType updatedAt }
            ... on ProjectV2SingleSelectField { id name dataType updatedAt options { id name } }
            ... on ProjectV2IterationField {
              id name dataType updatedAt
              configuration {
                iterations { id title startDate duration }
                completedIterations { id title startDate duration }
              }
            }
          }
          pageInfo { hasNextPage }
        }
        views(first: 100) {
          nodes { id number name layout filter }
          pageInfo { hasNextPage }
        }
        items(first: 100) {
          nodes {
            id type updatedAt
            content {
              __typename
              ... on Issue { id number title url repository { nameWithOwner } }
              ... on PullRequest { id number title url repository { nameWithOwner } }
              ... on DraftIssue { id title body }
            }
            fieldValues(first: 100) {
              nodes {
                __typename
                ... on ProjectV2ItemFieldTextValue {
                  field { ... on ProjectV2FieldCommon { id name } }
                  text updatedAt
                }
                ... on ProjectV2ItemFieldNumberValue {
                  field { ... on ProjectV2FieldCommon { id name } }
                  number updatedAt
                }
                ... on ProjectV2ItemFieldDateValue {
                  field { ... on ProjectV2FieldCommon { id name } }
                  date updatedAt
                }
                ... on ProjectV2ItemFieldSingleSelectValue {
                  field { ... on ProjectV2FieldCommon { id name } }
                  optionId name updatedAt
                }
                ... on ProjectV2ItemFieldIterationValue {
                  field { ... on ProjectV2FieldCommon { id name } }
                  iterationId title startDate duration updatedAt
                }
              }
              pageInfo { hasNextPage }
            }
          }
          pageInfo { hasNextPage }
        }
    "#
}

fn graphql(
    adapter: &GitHubAdapter,
    token: &str,
    query: &str,
    variables: Value,
) -> Result<Value, String> {
    let response = adapter
        .client
        .post(adapter.api_url("/graphql"))
        .bearer_auth(token)
        .json(&json!({ "query": query, "variables": variables }))
        .send()
        .map_err(|error| format!("GitHub Projects GraphQL request failed: {error}"))?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Ok(json!({ "errors": [{ "message": "projects credential rejected" }] }));
    }
    if !status.is_success() {
        let body = response
            .text()
            .unwrap_or_else(|_| "<unreadable response body>".into());
        return Err(format!("GitHub Projects GraphQL returned {status}: {body}"));
    }
    response
        .json()
        .map_err(|error| format!("invalid GitHub Projects GraphQL response: {error}"))
}

fn graphql_errors(payload: &Value) -> Result<(), String> {
    if let Some(errors) = payload.get("errors").and_then(Value::as_array) {
        if !errors.is_empty() {
            return Err(format!("GitHub Projects GraphQL returned errors: {errors:?}"));
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct ProviderProject {
    number: u64,
    node_id: String,
    url: String,
    owner_node_id: String,
    title: String,
    short_description: String,
    closed: bool,
    remote_updated_at: String,
    fields: Vec<ObservedProviderField>,
    items: Vec<ObservedProviderItem>,
    views: Vec<ObservedProviderView>,
}

impl ProviderProject {
    fn into_snapshot(
        self,
        canonical_id: &str,
        observed_at: &str,
    ) -> Result<ObservedProjectProviderState, String> {
        Ok(ObservedProjectProviderState {
            schema: OBSERVED_PROJECT_PROVIDER_STATE_SCHEMA_V0.into(),
            canonical_id: canonical_id.into(),
            number: self.number,
            node_id: self.node_id,
            url: self.url,
            owner_node_id: self.owner_node_id,
            title: self.title,
            short_description: self.short_description,
            closed: self.closed,
            remote_updated_at: self.remote_updated_at,
            observed_at: observed_at.into(),
            fields: self.fields,
            items: self.items,
            views: self.views,
        })
    }
}

fn parse_optional_project(value: Option<&Value>) -> Result<Option<ProviderProject>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(parse_project(value)?))
}

fn parse_project(value: &Value) -> Result<ProviderProject, String> {
    reject_truncated_connection(value.get("fields"), "fields")?;
    reject_truncated_connection(value.get("views"), "views")?;
    reject_truncated_connection(value.get("items"), "items")?;
    let mut fields = value
        .pointer("/fields/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|field| !field.is_null())
        .map(parse_field)
        .collect::<Result<Vec<_>, _>>()?;
    fields.sort_by(|left, right| left.node_id.cmp(&right.node_id));
    let mut views = value
        .pointer("/views/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|view| !view.is_null())
        .map(parse_view)
        .collect::<Result<Vec<_>, _>>()?;
    views.sort_by(|left, right| left.node_id.cmp(&right.node_id));
    let mut items = value
        .pointer("/items/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| !item.is_null())
        .map(parse_item)
        .collect::<Result<Vec<_>, _>>()?;
    items.sort_by(|left, right| left.node_id.cmp(&right.node_id));
    Ok(ProviderProject {
        number: required_u64(value, "number")?,
        node_id: required_string(value, "id")?,
        url: required_string(value, "url")?,
        owner_node_id: value
            .pointer("/owner/id")
            .and_then(Value::as_str)
            .ok_or_else(|| "ProjectV2 observation is missing owner.id".to_string())?
            .into(),
        title: required_string(value, "title")?,
        short_description: value
            .get("shortDescription")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        closed: value.get("closed").and_then(Value::as_bool).unwrap_or(false),
        remote_updated_at: required_string(value, "updatedAt")?,
        fields,
        items,
        views,
    })
}

fn parse_field(value: &Value) -> Result<ObservedProviderField, String> {
    let provider_type = required_string(value, "__typename")?;
    let mut options = value
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|option| {
            Ok(ObservedProviderOption {
                id: required_string(option, "id")?,
                name: required_string(option, "name")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    options.sort_by(|left, right| left.id.cmp(&right.id));
    let mut iterations = Vec::new();
    if let Some(configuration) = value.get("configuration") {
        for (key, completed) in [("iterations", false), ("completedIterations", true)] {
            for iteration in configuration
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                iterations.push(ObservedProviderIteration {
                    id: required_string(iteration, "id")?,
                    title: required_string(iteration, "title")?,
                    start_date: required_string(iteration, "startDate")?,
                    duration: iteration.get("duration").and_then(Value::as_i64).unwrap_or(0),
                    completed,
                });
            }
        }
    }
    iterations.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(ObservedProviderField {
        node_id: required_string(value, "id")?,
        provider_type,
        name: required_string(value, "name")?,
        data_type: required_string(value, "dataType")?,
        remote_updated_at: required_string(value, "updatedAt")?,
        options,
        iterations,
    })
}

fn parse_view(value: &Value) -> Result<ObservedProviderView, String> {
    Ok(ObservedProviderView {
        node_id: required_string(value, "id")?,
        number: required_u64(value, "number")?,
        name: required_string(value, "name")?,
        layout: required_string(value, "layout")?,
        filter: value.get("filter").and_then(Value::as_str).map(str::to_owned),
    })
}

fn parse_item(value: &Value) -> Result<ObservedProviderItem, String> {
    reject_truncated_connection(value.get("fieldValues"), "item fieldValues")?;
    let mut values = value
        .pointer("/fieldValues/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|field_value| !field_value.is_null())
        .filter_map(parse_field_value)
        .collect::<Result<Vec<_>, _>>()?;
    values.sort_by(|left, right| {
        left.field_node_id
            .cmp(&right.field_node_id)
            .then_with(|| left.provider_type.cmp(&right.provider_type))
    });
    Ok(ObservedProviderItem {
        node_id: required_string(value, "id")?,
        item_type: required_string(value, "type")?,
        remote_updated_at: required_string(value, "updatedAt")?,
        content: parse_content(value.get("content"))?,
        values,
    })
}

fn parse_content(value: Option<&Value>) -> Result<Option<ObservedProviderContent>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let provider_type = required_string(value, "__typename")?;
    let node_id = required_string(value, "id")?;
    match provider_type.as_str() {
        "Issue" | "PullRequest" => Ok(Some(ObservedProviderContent {
            provider_type,
            node_id,
            number: value.get("number").and_then(Value::as_u64),
            repository: value
                .pointer("/repository/nameWithOwner")
                .and_then(Value::as_str)
                .map(str::to_owned),
            title: value
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            url: value.get("url").and_then(Value::as_str).map(str::to_owned),
            body: None,
        })),
        "DraftIssue" => Ok(Some(ObservedProviderContent {
            provider_type,
            node_id,
            number: None,
            repository: None,
            title: value
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            url: None,
            body: value.get("body").and_then(Value::as_str).map(str::to_owned),
        })),
        _ => Ok(Some(ObservedProviderContent {
            provider_type,
            node_id,
            number: None,
            repository: None,
            title: String::new(),
            url: None,
            body: None,
        })),
    }
}

fn parse_field_value(value: &Value) -> Option<Result<ObservedProviderFieldValue, String>> {
    let provider_type = value.get("__typename")?.as_str()?.to_owned();
    let field_node_id = value.pointer("/field/id")?.as_str()?.to_owned();
    let field_name = value
        .pointer("/field/name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let remote_updated_at = value
        .get("updatedAt")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let normalized = match provider_type.as_str() {
        "ProjectV2ItemFieldTextValue" => value
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        "ProjectV2ItemFieldNumberValue" => value
            .get("number")
            .map(Value::to_string)
            .unwrap_or_default(),
        "ProjectV2ItemFieldDateValue" => value
            .get("date")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        "ProjectV2ItemFieldSingleSelectValue" => value
            .get("optionId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        "ProjectV2ItemFieldIterationValue" => value
            .get("iterationId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        _ => return None,
    };
    Some(Ok(ObservedProviderFieldValue {
        provider_type,
        field_node_id,
        field_name,
        value: normalized,
        remote_updated_at,
    }))
}

fn reject_truncated_connection(value: Option<&Value>, name: &str) -> Result<(), String> {
    if value
        .and_then(|connection| connection.pointer("/pageInfo/hasNextPage"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(format!(
            "GitHub ProjectV2 {name} exceeds the v0 observation page cap of 100; refusing to persist a silently truncated provider snapshot"
        ));
    }
    Ok(())
}

fn required_string(value: &Value, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("ProjectV2 observation is missing string field {key:?}"))
}

fn required_u64(value: &Value, key: &str) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("ProjectV2 observation is missing integer field {key:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use allodium_core::github_project::{
        load_observed_projects_capabilities, load_project_mappings,
    };
    use allodium_core::github_project_observation::load_observed_project_provider_state;
    use reqwest::blocking::Client;
    use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, USER_AGENT};
    use std::fs;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn rejected_projects_credential_does_not_become_authority() {
        let root = test_root("rejected");
        write_config(&root, "ALLODIUM_TEST_PROJECTS_TOKEN_REJECTED", "managed", None);
        unsafe {
            env::set_var(
                "ALLODIUM_TEST_PROJECTS_TOKEN_REJECTED",
                "not-secret-in-test",
            );
        }
        let (base, handle) = response_server(vec![json_response(403, "{}")]);
        let adapter = test_adapter(base);
        let report = observe_projects(&adapter, &root, "github").unwrap();
        handle.join().unwrap();
        unsafe { env::remove_var("ALLODIUM_TEST_PROJECTS_TOKEN_REJECTED") };
        assert_eq!(report.capabilities_observed, 1);
        assert_eq!(report.projects_observed, 0);
        let observed = load_observed_projects_capabilities(&root, "github")
            .unwrap()
            .unwrap();
        assert!(observed.credential_available);
        assert!(!observed.authenticated);
        assert!(observed.owner_node_id.is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn successful_projects_probe_records_only_owner_identity_not_token() {
        let root = test_root("success");
        write_config(&root, "ALLODIUM_TEST_PROJECTS_TOKEN_SUCCESS", "managed", None);
        unsafe { env::set_var("ALLODIUM_TEST_PROJECTS_TOKEN_SUCCESS", "not-secret-in-test") };
        let body = r#"{"data":{"user":{"id":"U_kgDOtest","projectsV2":{"totalCount":0}}}}"#;
        let (base, handle) = response_server(vec![json_response(200, body)]);
        let adapter = test_adapter(base);
        observe_projects(&adapter, &root, "github").unwrap();
        handle.join().unwrap();
        unsafe { env::remove_var("ALLODIUM_TEST_PROJECTS_TOKEN_SUCCESS") };
        let path = root.join(".project/remotes/github/observed/projects/capabilities.toml");
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("authenticated = true"));
        assert!(text.contains("owner_node_id = \"U_kgDOtest\""));
        assert!(!text.contains("not-secret-in-test"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_existing_project_observation_preserves_draft_and_provider_taxonomy() {
        let root = test_root("existing");
        write_config(
            &root,
            "ALLODIUM_TEST_PROJECTS_TOKEN_EXISTING",
            "existing",
            Some(42),
        );
        unsafe { env::set_var("ALLODIUM_TEST_PROJECTS_TOKEN_EXISTING", "token") };
        let probe = r#"{"data":{"user":{"id":"U_owner","projectsV2":{"totalCount":1}}}}"#;
        let project = provider_project_json("Todo", "Draft from provider", "2026-09-17T20:00:00Z");
        let (base, handle) = response_server(vec![json_response(200, probe), json_response(200, &project)]);
        let adapter = test_adapter(base);
        let report = observe_projects(&adapter, &root, "github").unwrap();
        handle.join().unwrap();
        unsafe { env::remove_var("ALLODIUM_TEST_PROJECTS_TOKEN_EXISTING") };
        assert_eq!(report.projects_observed, 1);
        let mappings = load_project_mappings(&root, "github").unwrap();
        assert_eq!(mappings.boards["board-0001"].number, 42);
        let snapshot = load_observed_project_provider_state(&root, "github", "board-0001")
            .unwrap()
            .unwrap();
        assert!(snapshot.items.iter().any(|item| item.item_type == "DRAFT_ISSUE"));
        assert!(snapshot.fields[0].iterations.iter().any(|iteration| iteration.id == "iteration-1"));
        assert!(!root.join(".project/issues/provider-draft").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changed_foreign_project_state_is_archived_once_and_reobservation_is_idempotent() {
        let root = test_root("drift");
        write_config(
            &root,
            "ALLODIUM_TEST_PROJECTS_TOKEN_DRIFT",
            "existing",
            Some(42),
        );
        unsafe { env::set_var("ALLODIUM_TEST_PROJECTS_TOKEN_DRIFT", "token") };
        let probe = r#"{"data":{"user":{"id":"U_owner","projectsV2":{"totalCount":1}}}}"#;
        let first = provider_project_json("Todo", "Draft one", "2026-09-17T20:00:00Z");
        let (base, handle) = response_server(vec![json_response(200, probe), json_response(200, &first)]);
        observe_projects(&test_adapter(base), &root, "github").unwrap();
        handle.join().unwrap();

        let second = provider_project_json("Doing", "Draft edited", "2026-09-17T20:02:00Z");
        let (base, handle) = response_server(vec![json_response(200, probe), json_response(200, &second)]);
        let changed = observe_projects(&test_adapter(base), &root, "github").unwrap();
        handle.join().unwrap();
        assert_eq!(changed.provider_changes_archived, 1);

        let (base, handle) = response_server(vec![json_response(200, probe), json_response(200, &second)]);
        let same = observe_projects(&test_adapter(base), &root, "github").unwrap();
        handle.join().unwrap();
        assert_eq!(same.provider_changes_archived, 0);
        unsafe { env::remove_var("ALLODIUM_TEST_PROJECTS_TOKEN_DRIFT") };
        let incoming = root.join(".project/remotes/github/incoming");
        let events = walk_named(&incoming, "event.toml");
        assert_eq!(events, 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn content_identity_is_recorded_only_for_objects_on_enabled_boards() {
        let root = test_root("content");
        write_config(&root, "ALLODIUM_UNUSED_PROJECTS_TOKEN", "managed", None);
        assert!(observe_repository_content_identity(
            &root,
            "github",
            "issue-0001",
            "issue",
            1,
            "I_issue",
            "2026-09-17T20:00:00Z",
        )
        .unwrap());
        assert!(!observe_repository_content_identity(
            &root,
            "github",
            "issue-9999",
            "issue",
            9999,
            "I_other",
            "2026-09-17T20:00:00Z",
        )
        .unwrap());
        assert!(root
            .join(".project/remotes/github/observed/projects/content/issue-0001.toml")
            .exists());
        assert!(!root
            .join(".project/remotes/github/observed/projects/content/issue-9999.toml")
            .exists());
        fs::remove_dir_all(root).unwrap();
    }

    fn provider_project_json(option_name: &str, draft_title: &str, updated_at: &str) -> String {
        json!({
            "data": {
                "user": {
                    "projectV2": {
                        "id": "PVT_project",
                        "number": 42,
                        "url": "https://github.com/users/sguzman/projects/42",
                        "title": "Board",
                        "shortDescription": "Provider board",
                        "closed": false,
                        "updatedAt": updated_at,
                        "owner": { "id": "U_owner" },
                        "fields": {
                            "nodes": [
                                {
                                    "__typename": "ProjectV2SingleSelectField",
                                    "id": "PVTSSF_status",
                                    "name": "Status",
                                    "dataType": "SINGLE_SELECT",
                                    "updatedAt": updated_at,
                                    "options": [{ "id": "option-1", "name": option_name }]
                                },
                                {
                                    "__typename": "ProjectV2IterationField",
                                    "id": "PVTIF_iteration",
                                    "name": "Iteration",
                                    "dataType": "ITERATION",
                                    "updatedAt": updated_at,
                                    "configuration": {
                                        "iterations": [{ "id": "iteration-1", "title": "Sprint 1", "startDate": "2026-09-14", "duration": 7 }],
                                        "completedIterations": []
                                    }
                                }
                            ],
                            "pageInfo": { "hasNextPage": false }
                        },
                        "views": {
                            "nodes": [{ "id": "PVTV_view", "number": 1, "name": "Board", "layout": "BOARD_LAYOUT", "filter": "" }],
                            "pageInfo": { "hasNextPage": false }
                        },
                        "items": {
                            "nodes": [{
                                "id": "PVTI_draft",
                                "type": "DRAFT_ISSUE",
                                "updatedAt": updated_at,
                                "content": { "__typename": "DraftIssue", "id": "DI_draft", "title": draft_title, "body": "provider-only body" },
                                "fieldValues": { "nodes": [], "pageInfo": { "hasNextPage": false } }
                            }],
                            "pageInfo": { "hasNextPage": false }
                        }
                    }
                }
            }
        })
        .to_string()
    }

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-projects-runtime-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join(".project/issues/issue-0001")).unwrap();
        fs::create_dir_all(root.join(".project/remotes/github")).unwrap();
        fs::create_dir_all(root.join(".project/boards/board-0001/items")).unwrap();
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
            root.join(".project/boards/board-0001/board.toml"),
            "schema = \"allodium.board/v0\"\nid = \"board-0001\"\ntitle = \"Board\"\n",
        )
        .unwrap();
        fs::write(
            root.join(".project/boards/board-0001/items/issue-0001.toml"),
            "schema = \"allodium.board-item/v0\"\nobject = \"issue-0001\"\n",
        )
        .unwrap();
        fs::write(
            root.join(".project/remotes/github/remote.toml"),
            "schema = \"allodium.remote/v0\"\nname = \"github\"\nkind = \"github\"\nrepository = \"sguzman/allodium\"\noutbound = \"reconcile\"\ninbound = \"archive\"\n",
        )
        .unwrap();
        root
    }

    fn write_config(root: &Path, credential_env: &str, target: &str, project_number: Option<u64>) {
        let number = project_number
            .map(|number| format!("project_number = {number}\n"))
            .unwrap_or_default();
        fs::write(
            root.join(".project/remotes/github/projects.toml"),
            format!(
                "schema = \"allodium.github.projects-projection/v0\"\nowner_kind = \"user\"\nowner = \"sguzman\"\ncredential_env = \"{credential_env}\"\n\n[boards.board-0001]\nenabled = true\ntarget = \"{target}\"\n{number}"
            ),
        )
        .unwrap();
    }

    fn test_adapter(api_base: String) -> GitHubAdapter {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("allodium-test"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        GitHubAdapter {
            client: Client::builder().default_headers(headers).build().unwrap(),
            repository: "sguzman/allodium".into(),
            token: None,
            api_base,
        }
    }

    fn response_server(responses: Vec<String>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                read_request(&mut stream);
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        (format!("http://{address}"), handle)
    }

    fn json_response(status: u16, body: &str) -> String {
        let reason = if status == 200 { "OK" } else { "Forbidden" };
        format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(), body
        )
    }

    fn read_request(stream: &mut TcpStream) {
        let mut buffer = [0_u8; 8192];
        let mut received = Vec::new();
        loop {
            let count = stream.read(&mut buffer).unwrap();
            if count == 0 {
                break;
            }
            received.extend_from_slice(&buffer[..count]);
            if received.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
    }

    fn walk_named(root: &Path, name: &str) -> usize {
        if !root.exists() {
            return 0;
        }
        let mut count = 0;
        let mut stack = vec![root.to_path_buf()];
        while let Some(path) = stack.pop() {
            for entry in fs::read_dir(path).unwrap() {
                let entry = entry.unwrap();
                if entry.path().is_dir() {
                    stack.push(entry.path());
                } else if entry.file_name() == name {
                    count += 1;
                }
            }
        }
        count
    }
}
'''

Path("crates/allodium-core/src/github_project_observation.rs").write_text(CORE_OBSERVATION)
Path("crates/allodium-github/src/project.rs").write_text(PROJECT_RUNTIME)

core_lib = Path("crates/allodium-core/src/lib.rs")
text = core_lib.read_text()
needle = "pub mod github_project;\n"
if "pub mod github_project_observation;" not in text:
    if needle not in text:
        raise SystemExit("github_project module declaration not found")
    text = text.replace(needle, needle + "pub mod github_project_observation;\n", 1)
core_lib.write_text(text)

adapter = Path("crates/allodium-github/src/lib.rs")
text = adapter.read_text()
text = text.replace(
    "    pub projects_capabilities_observed: usize,\n",
    "    pub projects_capabilities_observed: usize,\n    pub projects_observed: usize,\n    pub projects_provider_changes_archived: usize,\n    pub projects_item_identities_mapped: usize,\n    pub projects_content_identities_observed: usize,\n",
    1,
)
text = text.replace(
    "struct ApiIssue {\n    number: u64,\n",
    "struct ApiIssue {\n    number: u64,\n    #[serde(default)]\n    node_id: String,\n",
    1,
)
text = text.replace(
    "struct ApiPullRequest {\n    number: u64,\n",
    "struct ApiPullRequest {\n    number: u64,\n    #[serde(default)]\n    node_id: String,\n",
    1,
)
issue_needle = "            write_observed_issue(root, remote_name, &canonical_id, &issue, &observed_at)?;\n            report.issues_observed += 1;\n"
issue_replacement = "            write_observed_issue(root, remote_name, &canonical_id, &issue, &observed_at)?;\n            if project::observe_repository_content_identity(\n                root, remote_name, &canonical_id, \"issue\", issue.number, &issue.node_id, &observed_at,\n            )? {\n                report.projects_content_identities_observed += 1;\n            }\n            report.issues_observed += 1;\n"
if issue_needle not in text:
    raise SystemExit("issue observation insertion point not found")
text = text.replace(issue_needle, issue_replacement, 1)
review_needle = "            write_observed_review(root, remote_name, &canonical_id, &review, &observed_at)?;\n            report.reviews_observed += 1;\n"
review_replacement = "            write_observed_review(root, remote_name, &canonical_id, &review, &observed_at)?;\n            if project::observe_repository_content_identity(\n                root, remote_name, &canonical_id, \"pull_request\", review.number, &review.node_id, &observed_at,\n            )? {\n                report.projects_content_identities_observed += 1;\n            }\n            report.reviews_observed += 1;\n"
if review_needle not in text:
    raise SystemExit("review observation insertion point not found")
text = text.replace(review_needle, review_replacement, 1)
projects_needle = "        let projects_report = project::observe_projects_capabilities(self, root, remote_name)?;\n        report.projects_capabilities_observed += projects_report.capabilities_observed;\n"
projects_replacement = "        let projects_report = project::observe_projects(self, root, remote_name)?;\n        report.projects_capabilities_observed += projects_report.capabilities_observed;\n        report.projects_observed += projects_report.projects_observed;\n        report.projects_provider_changes_archived += projects_report.provider_changes_archived;\n        report.projects_item_identities_mapped += projects_report.item_identities_mapped;\n"
if projects_needle not in text:
    raise SystemExit("Projects observation call insertion point not found")
text = text.replace(projects_needle, projects_replacement, 1)
adapter.write_text(text)

cli = Path("crates/allodium-cli/src/main.rs")
text = cli.read_text()
needle = '''            println!(
                "observed {} GitHub Projects capability snapshot(s)",
                report.projects_capabilities_observed
            );
'''
replacement = needle + '''            println!("observed {} GitHub ProjectV2 project(s)", report.projects_observed);
            println!(
                "observed {} GitHub Project content node identit(y/ies)",
                report.projects_content_identities_observed
            );
            println!(
                "mapped {} GitHub Project item identit(y/ies)",
                report.projects_item_identities_mapped
            );
            println!(
                "archived {} GitHub Project provider-state change(s)",
                report.projects_provider_changes_archived
            );
'''
if needle not in text:
    raise SystemExit("CLI Projects observe print insertion point not found")
cli.write_text(text.replace(needle, replacement, 1))

doc = Path("docs/github-projects-provider-v0.md")
text = doc.read_text()
addition = '''

## Read-only provider observation

When the separately configured Projects credential authenticates the explicit owner, Allodium may observe an explicitly numbered existing Project or a Project already bound by stable node ID. The v0 observer persists normalized provider snapshots for project metadata, fields, single-select options, iterations, views, items, supported field values, and Issue/PullRequest/DraftIssue content. Provider DraftIssue content remains provider evidence and is never promoted into `.project/issues/`.

A changed provider snapshot archives both the previous and current observations under the GitHub remote `incoming/` namespace before the current observation is replaced. Re-observing semantically identical state is idempotent. Project connections are capped at 100 objects in v0; if GitHub reports another page, observation fails rather than silently persisting a truncated snapshot.

Ordinary mapped issue and pull-request observations also persist their GitHub GraphQL node IDs when those canonical objects belong to an enabled Allodium board. This supplies the stable content identity required for later ProjectV2 item membership without requiring Project authorization merely to learn repository-object identity.
'''
if "## Read-only provider observation" not in text:
    text += addition
doc.write_text(text)
