use crate::board::{CanonicalBoard, load_boards};
use crate::github::{
    GitHubOperation, GitHubPlan, ISSUE_MAPPINGS_SCHEMA_V0, IssueMappings, PLAN_SCHEMA_V0,
    REVIEW_MAPPINGS_SCHEMA_V0, ReviewMappings,
};
use crate::github_project_observation::load_observed_project_provider_state;
use crate::load_remote;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const PROJECTS_PROJECTION_SCHEMA_V0: &str = "allodium.github.projects-projection/v0";
pub const PROJECT_MAPPINGS_SCHEMA_V0: &str = "allodium.github.project-mappings/v0";
pub const OBSERVED_PROJECTS_CAPABILITIES_SCHEMA_V0: &str =
    "allodium.github.observed-projects-capabilities/v0";
pub const OBSERVED_PROJECT_SCHEMA_V0: &str = "allodium.github.observed-project/v0";
pub const OBSERVED_PROJECT_CONTENT_SCHEMA_V0: &str = "allodium.github.observed-project-content/v0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectBoardBinding {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_number: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectsProjectionConfig {
    pub schema: String,
    pub owner_kind: String,
    pub owner: String,
    pub credential_env: String,
    #[serde(default)]
    pub boards: BTreeMap<String, ProjectBoardBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectItemMapping {
    pub node_id: String,
    pub content_node_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectFieldMapping {
    pub node_id: String,
    pub data_type: String,
    #[serde(default)]
    pub options: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectViewMapping {
    pub node_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectMapping {
    pub number: u64,
    pub node_id: String,
    pub url: String,
    pub owner_node_id: String,
    #[serde(default)]
    pub items: BTreeMap<String, ProjectItemMapping>,
    #[serde(default)]
    pub fields: BTreeMap<String, ProjectFieldMapping>,
    #[serde(default)]
    pub views: BTreeMap<String, ProjectViewMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectMappings {
    pub schema: String,
    #[serde(default)]
    pub boards: BTreeMap<String, ProjectMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedProjectsCapabilities {
    pub schema: String,
    pub credential_available: bool,
    pub authenticated: bool,
    pub owner_kind: String,
    pub owner: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_node_id: Option<String>,
    pub observed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedProject {
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedProjectContentIdentity {
    pub schema: String,
    pub canonical_id: String,
    pub remote_type: String,
    pub number: u64,
    pub node_id: String,
    pub observed_at: String,
}

pub fn plan_boards(root: impl AsRef<Path>, remote_name: &str) -> Result<GitHubPlan, String> {
    let root = root.as_ref();
    let remote = load_remote(root, remote_name)?;
    if remote.kind != "github" {
        return Err(format!(
            "remote {remote_name:?} has kind {:?}, not \"github\"",
            remote.kind
        ));
    }

    let boards = load_boards(root)?;
    let mut operations = Vec::new();
    if boards.is_empty() {
        return Ok(plan(remote_name, remote.repository, operations));
    }

    let Some(config) = load_projects_projection_config(root, remote_name)? else {
        operations.push(GitHubOperation {
            canonical_id: "boards".into(),
            action: "projects_config_required".into(),
            number: None,
            fields: vec![
                "owner".into(),
                "credential_env".into(),
                "board_bindings".into(),
                "target".into(),
            ],
            reason:
                "canonical boards exist but GitHub Projects projection configuration is missing"
                    .into(),
        });
        return Ok(plan(remote_name, remote.repository, operations));
    };

    let mut missing_binding = false;
    for board in &boards {
        if !config.boards.contains_key(&board.record.id) {
            missing_binding = true;
            operations.push(GitHubOperation {
                canonical_id: board.record.id.clone(),
                action: "projects_config_required".into(),
                number: None,
                fields: vec!["board_binding".into(), "target".into()],
                reason: "canonical board has no explicit GitHub Projects projection binding".into(),
            });
        }
    }
    if missing_binding {
        return Ok(plan(remote_name, remote.repository, operations));
    }

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
        return Ok(plan(remote_name, remote.repository, operations));
    }

    let Some(capabilities) = load_observed_projects_capabilities(root, remote_name)? else {
        operations.push(GitHubOperation {
            canonical_id: "projects".into(),
            action: "observe_projects_capabilities".into(),
            number: None,
            fields: vec!["credential".into(), "owner".into()],
            reason: "GitHub Projects capability observation is missing".into(),
        });
        return Ok(plan(remote_name, remote.repository, operations));
    };

    if !capabilities.credential_available || !capabilities.authenticated {
        operations.push(GitHubOperation {
            canonical_id: "projects".into(),
            action: "projects_auth_required".into(),
            number: None,
            fields: vec![config.credential_env.clone()],
            reason: format!(
                "GitHub Projects requires separately authorized credentials via {}; repository GITHUB_TOKEN authority is intentionally insufficient",
                config.credential_env
            ),
        });
        return Ok(plan(remote_name, remote.repository, operations));
    }

    if capabilities.owner_kind != config.owner_kind
        || capabilities.owner != config.owner
        || capabilities.owner_node_id.is_none()
    {
        operations.push(GitHubOperation {
            canonical_id: "projects".into(),
            action: "projects_owner_review_required".into(),
            number: None,
            fields: vec!["owner_kind".into(), "owner".into(), "owner_node_id".into()],
            reason:
                "observed GitHub Projects authority does not match the explicitly configured owner"
                    .into(),
        });
        return Ok(plan(remote_name, remote.repository, operations));
    }

    let mappings = load_project_mappings(root, remote_name)?;
    let issue_mappings = load_issue_mappings(root, remote_name)?;
    let review_mappings = load_review_mappings(root, remote_name)?;
    let owner_node_id = capabilities
        .owner_node_id
        .as_deref()
        .expect("owner node ID was checked above");

    for board in enabled {
        let binding = config
            .boards
            .get(&board.record.id)
            .expect("board bindings were checked above");
        plan_board(
            root,
            remote_name,
            board,
            binding,
            owner_node_id,
            &mappings,
            &issue_mappings,
            &review_mappings,
            &mut operations,
        )?;
    }

    Ok(plan(remote_name, remote.repository, operations))
}

#[allow(clippy::too_many_arguments)]
fn plan_board(
    root: &Path,
    remote_name: &str,
    board: &CanonicalBoard,
    binding: &ProjectBoardBinding,
    owner_node_id: &str,
    mappings: &ProjectMappings,
    issue_mappings: &IssueMappings,
    review_mappings: &ReviewMappings,
    operations: &mut Vec<GitHubOperation>,
) -> Result<(), String> {
    let mapping = mappings.boards.get(&board.record.id);
    let Some(mapping) = mapping else {
        match binding.target.as_str() {
            "managed" => operations.push(project_operation(
                &board.record.id,
                "create_project",
                None,
                vec!["project:create".into()],
                "explicit managed target has no provider project mapping; create exactly one ProjectV2 before any dependent mutation",
            )),
            "existing" => operations.push(project_operation(
                &board.record.id,
                "bind_project",
                binding.project_number,
                vec!["project:bind-existing".into()],
                "explicit existing ProjectV2 target must be bound by configured number and stable provider identity; title matching is forbidden",
            )),
            _ => unreachable!("projection config target was validated"),
        }
        return Ok(());
    };

    if binding.target == "existing" && binding.project_number != Some(mapping.number) {
        operations.push(runtime_requirement(
            &board.record.id,
            Some(mapping.number),
            vec!["project:identity-review".into()],
            "mapped ProjectV2 number does not match the explicitly configured existing target; refusing implicit retargeting",
        ));
        return Ok(());
    }
    if mapping.owner_node_id != owner_node_id {
        operations.push(runtime_requirement(
            &board.record.id,
            Some(mapping.number),
            vec!["project:owner-identity-review".into()],
            "mapped ProjectV2 owner identity does not match the authenticated configured owner",
        ));
        return Ok(());
    }

    let Some(observed) = load_observed_project(root, remote_name, &board.record.id)? else {
        operations.push(project_operation(
            &board.record.id,
            "observe_project",
            Some(mapping.number),
            vec!["project:observe".into()],
            "mapped ProjectV2 has no persisted observation; observation is required before any mutable work",
        ));
        return Ok(());
    };
    if observed.number != mapping.number
        || observed.node_id != mapping.node_id
        || observed.owner_node_id != mapping.owner_node_id
    {
        operations.push(runtime_requirement(
            &board.record.id,
            Some(mapping.number),
            vec!["project:identity-review".into()],
            "observed ProjectV2 identity disagrees with the stable mapping; refusing automatic repair",
        ));
        return Ok(());
    }

    let Some(provider_state) =
        load_observed_project_provider_state(root, remote_name, &board.record.id)?
    else {
        operations.push(project_operation(
            &board.record.id,
            "observe_project",
            Some(mapping.number),
            vec!["project:provider-state".into()],
            "full ProjectV2 provider-state observation is required before mutation",
        ));
        return Ok(());
    };
    if provider_state.node_id != mapping.node_id || provider_state.number != mapping.number {
        operations.push(runtime_requirement(
            &board.record.id,
            Some(mapping.number),
            vec!["project:provider-state-identity-review".into()],
            "full ProjectV2 provider-state observation disagrees with the stable mapping",
        ));
        return Ok(());
    }

    let mut project_fields = Vec::new();
    if observed.title != board.record.title {
        project_fields.push("title".into());
    }
    if observed.short_description != board.record.description {
        project_fields.push("short_description".into());
    }
    if observed.closed {
        project_fields.push("closed".into());
    }
    if !project_fields.is_empty() {
        operations.push(project_operation(
            &board.record.id,
            "update_project",
            Some(mapping.number),
            project_fields,
            "canonical board metadata differs from the last observed ProjectV2 state; apply only after a fresh optimistic identity/state check",
        ));
        return Ok(());
    }

    // Content node identity can be established independently of Project field
    // schema. Diagnose missing/review-required repository identity first, but
    // do not add membership until provider field identity is settled below.
    for item in &board.items {
        let canonical_id = &item.record.object;
        let Some((remote_type, number)) =
            provider_object(canonical_id, issue_mappings, review_mappings)
        else {
            operations.push(runtime_requirement(
                &board.record.id,
                Some(mapping.number),
                vec![format!("repository-identity:{canonical_id}")],
                "canonical board item has no repository projection identity",
            ));
            return Ok(());
        };
        let Some(content) = load_observed_project_content(root, remote_name, canonical_id)? else {
            operations.push(runtime_requirement(
                &board.record.id,
                Some(mapping.number),
                vec![format!("content-identity:{canonical_id}")],
                "canonical board item lacks the persisted GitHub GraphQL content node identity required by addProjectV2ItemById",
            ));
            return Ok(());
        };
        if content.remote_type != remote_type
            || content.number != number
            || content.node_id.trim().is_empty()
        {
            operations.push(runtime_requirement(
                &board.record.id,
                Some(mapping.number),
                vec![format!("content-identity-review:{canonical_id}")],
                "persisted board-item content identity disagrees with the repository mapping",
            ));
            return Ok(());
        }
    }

    // Establish provider field identity before item values. Managed targets may
    // create namespaced provider fields; existing targets never guess by name.
    for field in &board.fields {
        let expected_type = canonical_provider_field_type(&field.record.kind)?;
        let Some(field_mapping) = mapping.fields.get(&field.record.id) else {
            if binding.target == "managed" {
                operations.push(project_operation(
                    &board.record.id,
                    "create_project_field",
                    Some(mapping.number),
                    vec![format!("field:{}", field.record.id)],
                    "managed ProjectV2 is missing a stable provider field mapping; create one provider field and observe before dependent values",
                ));
            } else {
                operations.push(runtime_requirement(
                    &board.record.id,
                    Some(mapping.number),
                    vec![format!("field-binding:{}", field.record.id)],
                    "existing ProjectV2 has no explicit stable field mapping; refusing name-based field identity",
                ));
            }
            return Ok(());
        };
        if field_mapping.node_id.trim().is_empty() || field_mapping.data_type != expected_type {
            operations.push(runtime_requirement(
                &board.record.id,
                Some(mapping.number),
                vec![format!("field-identity-review:{}", field.record.id)],
                "persisted ProjectV2 field identity or provider data type disagrees with the canonical field contract",
            ));
            return Ok(());
        }
        if field.record.kind == "single_select" {
            for option in &field.record.options {
                if !field_mapping.options.contains_key(&option.id) {
                    operations.push(runtime_requirement(
                        &board.record.id,
                        Some(mapping.number),
                        vec![format!("field-option:{}:{}", field.record.id, option.id)],
                        "canonical single-select option has no stable provider option identity; option-schema mutation is intentionally not guessed",
                    ));
                    return Ok(());
                }
            }
        }
    }

    // Membership is a separate mutation from field values. A missing item map
    // therefore produces exactly one add operation and returns immediately.
    for item in &board.items {
        let canonical_id = &item.record.object;
        let Some((remote_type, number)) =
            provider_object(canonical_id, issue_mappings, review_mappings)
        else {
            operations.push(runtime_requirement(
                &board.record.id,
                Some(mapping.number),
                vec![format!("repository-identity:{canonical_id}")],
                "canonical board item has no repository projection identity",
            ));
            return Ok(());
        };
        let Some(content) = load_observed_project_content(root, remote_name, canonical_id)? else {
            operations.push(runtime_requirement(
                &board.record.id,
                Some(mapping.number),
                vec![format!("content-identity:{canonical_id}")],
                "canonical board item lacks the persisted GitHub GraphQL content node identity required by addProjectV2ItemById",
            ));
            return Ok(());
        };
        if content.remote_type != remote_type
            || content.number != number
            || content.node_id.trim().is_empty()
        {
            operations.push(runtime_requirement(
                &board.record.id,
                Some(mapping.number),
                vec![format!("content-identity-review:{canonical_id}")],
                "persisted board-item content identity disagrees with the repository mapping",
            ));
            return Ok(());
        }
        match mapping.items.get(canonical_id) {
            Some(item_mapping) if item_mapping.content_node_id == content.node_id => {}
            Some(_) => {
                operations.push(runtime_requirement(
                    &board.record.id,
                    Some(mapping.number),
                    vec![format!("item-identity-review:{canonical_id}")],
                    "persisted ProjectV2 item identity points at a different content node",
                ));
                return Ok(());
            }
            None => {
                operations.push(project_operation(
                    &board.record.id,
                    "add_project_item",
                    Some(mapping.number),
                    vec![format!("item:{canonical_id}")],
                    "ProjectV2 membership is missing; add the existing Issue/PullRequest by stable content node ID and re-observe before field values",
                ));
                return Ok(());
            }
        }
    }

    // Explicit canonical values are managed. Canonical absence does not clear
    // provider values in v0 because absence/delete semantics remain undeclared.
    for item in &board.items {
        let item_mapping = mapping
            .items
            .get(&item.record.object)
            .expect("membership loop established item mapping");
        let Some(provider_item) = provider_state
            .items
            .iter()
            .find(|candidate| candidate.node_id == item_mapping.node_id)
        else {
            operations.push(project_operation(
                &board.record.id,
                "observe_project",
                Some(mapping.number),
                vec![format!("item-provider-state:{}", item.record.object)],
                "mapped ProjectV2 item is absent from the persisted provider snapshot; refresh observation before mutation",
            ));
            return Ok(());
        };
        for (field_id, value) in &item.record.values {
            let field = board
                .fields
                .iter()
                .find(|field| field.record.id == *field_id)
                .expect("validated canonical board value references known field");
            let field_mapping = mapping
                .fields
                .get(field_id)
                .expect("field loop established provider field mapping");
            let expected = expected_provider_value(&field.record.kind, value, field_mapping)?;
            let observed_value = provider_item
                .values
                .iter()
                .find(|candidate| candidate.field_node_id == field_mapping.node_id)
                .map(|candidate| candidate.value.as_str());
            if observed_value != Some(expected.as_str()) {
                operations.push(project_operation(
                    &board.record.id,
                    "update_project_field_value",
                    Some(mapping.number),
                    vec![
                        format!("item:{}", item.record.object),
                        format!("field:{field_id}"),
                    ],
                    "canonical board field value differs from the last observed ProjectV2 item value; update only after a fresh optimistic state check",
                ));
                return Ok(());
            }
        }
    }

    for view in &board.views {
        let expected_layout = match view.record.layout.as_str() {
            "table" => "TABLE_LAYOUT",
            "board" => "BOARD_LAYOUT",
            "roadmap" => "ROADMAP_LAYOUT",
            other => return Err(format!("unsupported canonical board view layout {other:?}")),
        };
        let Some(view_mapping) = mapping.views.get(&view.record.id) else {
            if binding.target != "managed" {
                operations.push(runtime_requirement(
                    &board.record.id,
                    Some(mapping.number),
                    vec![format!("view-binding:{}", view.record.id)],
                    "existing ProjectV2 target has no explicit stable view mapping; refusing name-based view identity",
                ));
                return Ok(());
            }
            if view.record.layout == "roadmap" {
                operations.push(runtime_requirement(
                    &board.record.id,
                    Some(mapping.number),
                    vec![format!("view-semantics:{}", view.record.id)],
                    "canonical roadmap start/end-field semantics do not have an audited lossless GitHub Project view creation representation",
                ));
                return Ok(());
            }
            if view.record.layout == "board" {
                let field_id = view
                    .record
                    .group_by
                    .as_deref()
                    .expect("validated canonical board view has group_by");
                let field_mapping = mapping.fields.get(field_id).ok_or_else(|| {
                    format!(
                        "board view {:?} grouping field has no stable provider field mapping",
                        view.record.id
                    )
                })?;
                let Some(provider_field) = provider_state
                    .fields
                    .iter()
                    .find(|field| field.node_id == field_mapping.node_id)
                else {
                    operations.push(runtime_requirement(
                        &board.record.id,
                        Some(mapping.number),
                        vec![format!("view-field-observation:{}:{}", view.record.id, field_id)],
                        "canonical board view grouping field is mapped but absent from the persisted provider-state observation",
                    ));
                    return Ok(());
                };
                if provider_field.provider_database_id.is_none_or(|id| id <= 0) {
                    operations.push(runtime_requirement(
                        &board.record.id,
                        Some(mapping.number),
                        vec![format!("view-field-bridge:{}:{}", view.record.id, field_id)],
                        "canonical board view grouping requires the observed positive provider database field ID used by GitHub's REST view API",
                    ));
                    return Ok(());
                }
            }
            operations.push(project_operation(
                &board.record.id,
                "create_project_view",
                Some(mapping.number),
                vec![format!("view:{}", view.record.id)],
                "managed ProjectV2 is missing a stable provider view mapping and the canonical view has an audited lossless create-once representation",
            ));
            return Ok(());
        };

        let Some(provider_view) = provider_state
            .views
            .iter()
            .find(|view| view.node_id == view_mapping.node_id)
        else {
            operations.push(runtime_requirement(
                &board.record.id,
                Some(mapping.number),
                vec![format!("view-identity-review:{}", view.record.id)],
                "mapped ProjectV2 view is absent from the persisted provider-state observation",
            ));
            return Ok(());
        };
        if view.record.layout == "roadmap" {
            operations.push(runtime_requirement(
                &board.record.id,
                Some(mapping.number),
                vec![format!("view-semantics:{}", view.record.id)],
                "mapped roadmap view cannot yet be audited against canonical start/end-field semantics without loss",
            ));
            return Ok(());
        }
        if provider_view.name != view.record.name || provider_view.layout != expected_layout {
            operations.push(runtime_requirement(
                &board.record.id,
                Some(mapping.number),
                vec![format!("view-drift-review:{}", view.record.id)],
                "mapped ProjectV2 view name or layout drifted; v0 view projection is immutable-after-create and requires review",
            ));
            return Ok(());
        }
        if view.record.layout == "board" {
            let field_id = view
                .record
                .group_by
                .as_deref()
                .expect("validated canonical board view has group_by");
            let field_mapping = mapping.fields.get(field_id).ok_or_else(|| {
                format!(
                    "board view {:?} grouping field has no stable provider field mapping",
                    view.record.id
                )
            })?;
            if provider_view.vertical_group_by_field_node_ids != vec![field_mapping.node_id.clone()]
            {
                operations.push(runtime_requirement(
                    &board.record.id,
                    Some(mapping.number),
                    vec![format!("view-drift-review:{}", view.record.id)],
                    "mapped ProjectV2 board view columns no longer match the canonical group_by field; v0 refuses automatic view reconfiguration",
                ));
                return Ok(());
            }
        }
    }

    Ok(())
}

fn project_operation(
    canonical_id: &str,
    action: &str,
    number: Option<u64>,
    fields: Vec<String>,
    reason: &str,
) -> GitHubOperation {
    GitHubOperation {
        canonical_id: canonical_id.into(),
        action: action.into(),
        number,
        fields,
        reason: reason.into(),
    }
}

fn canonical_provider_field_type(kind: &str) -> Result<&'static str, String> {
    match kind {
        "text" => Ok("TEXT"),
        "number" => Ok("NUMBER"),
        "date" => Ok("DATE"),
        "single_select" => Ok("SINGLE_SELECT"),
        other => Err(format!("unsupported canonical board field kind {other:?}")),
    }
}

fn expected_provider_value(
    kind: &str,
    value: &toml::Value,
    mapping: &ProjectFieldMapping,
) -> Result<String, String> {
    match kind {
        "text" | "date" => value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("canonical {kind} field value is not a string")),
        "number" => {
            if let Some(integer) = value.as_integer() {
                Ok(integer.to_string())
            } else if let Some(number) = value.as_float() {
                Ok(number.to_string())
            } else {
                Err("canonical number field value is not numeric".into())
            }
        }
        "single_select" => {
            let option = value.as_str().ok_or_else(|| {
                "canonical single-select value is not a string option id".to_string()
            })?;
            mapping.options.get(option).cloned().ok_or_else(|| {
                format!("canonical option {option:?} has no provider option mapping")
            })
        }
        other => Err(format!("unsupported canonical board field kind {other:?}")),
    }
}

fn provider_object(
    canonical_id: &str,
    issue_mappings: &IssueMappings,
    review_mappings: &ReviewMappings,
) -> Option<(String, u64)> {
    if let Some(mapping) = issue_mappings.issues.get(canonical_id) {
        return Some(("issue".into(), mapping.number));
    }
    review_mappings
        .reviews
        .get(canonical_id)
        .map(|mapping| ("pull_request".into(), mapping.number))
}

fn runtime_requirement(
    canonical_id: &str,
    number: Option<u64>,
    fields: Vec<String>,
    reason: &str,
) -> GitHubOperation {
    GitHubOperation {
        canonical_id: canonical_id.into(),
        action: "projects_runtime_required".into(),
        number,
        fields,
        reason: reason.into(),
    }
}

pub fn load_projects_projection_config(
    root: impl AsRef<Path>,
    remote_name: &str,
) -> Result<Option<ProjectsProjectionConfig>, String> {
    let path = projects_projection_config_path(root.as_ref(), remote_name);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let config: ProjectsProjectionConfig =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    validate_projection_config(&path, &config)?;
    Ok(Some(config))
}

pub fn load_project_mappings(
    root: impl AsRef<Path>,
    remote_name: &str,
) -> Result<ProjectMappings, String> {
    let path = project_mappings_path(root.as_ref(), remote_name);
    if !path.exists() {
        return Ok(ProjectMappings {
            schema: PROJECT_MAPPINGS_SCHEMA_V0.into(),
            boards: BTreeMap::new(),
        });
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mappings: ProjectMappings =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if mappings.schema != PROJECT_MAPPINGS_SCHEMA_V0 {
        return Err(format!(
            "{}: unsupported GitHub Project mapping schema {:?}; expected {:?}",
            path.display(),
            mappings.schema,
            PROJECT_MAPPINGS_SCHEMA_V0
        ));
    }
    validate_project_mappings(&path, &mappings)?;
    Ok(mappings)
}

pub fn save_project_mappings(
    root: impl AsRef<Path>,
    remote_name: &str,
    mappings: &ProjectMappings,
) -> Result<(), String> {
    if mappings.schema != PROJECT_MAPPINGS_SCHEMA_V0 {
        return Err(format!(
            "refusing to write unsupported GitHub Project mapping schema {:?}",
            mappings.schema
        ));
    }
    let path = project_mappings_path(root.as_ref(), remote_name);
    validate_project_mappings(&path, mappings)?;
    fs::create_dir_all(path.parent().expect("Project mapping path has parent"))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let text = toml::to_string_pretty(mappings)
        .map_err(|error| format!("could not serialize GitHub Project mappings: {error}"))?;
    fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn load_observed_projects_capabilities(
    root: impl AsRef<Path>,
    remote_name: &str,
) -> Result<Option<ObservedProjectsCapabilities>, String> {
    let path = observed_projects_capabilities_path(root.as_ref(), remote_name);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let observed: ObservedProjectsCapabilities =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if observed.schema != OBSERVED_PROJECTS_CAPABILITIES_SCHEMA_V0 {
        return Err(format!(
            "{}: unsupported observed GitHub Projects capability schema {:?}; expected {:?}",
            path.display(),
            observed.schema,
            OBSERVED_PROJECTS_CAPABILITIES_SCHEMA_V0
        ));
    }
    Ok(Some(observed))
}

pub fn write_observed_projects_capabilities(
    root: impl AsRef<Path>,
    remote_name: &str,
    observed: &ObservedProjectsCapabilities,
) -> Result<(), String> {
    if observed.schema != OBSERVED_PROJECTS_CAPABILITIES_SCHEMA_V0 {
        return Err(format!(
            "refusing to write unsupported observed GitHub Projects capability schema {:?}",
            observed.schema
        ));
    }
    let path = observed_projects_capabilities_path(root.as_ref(), remote_name);
    fs::create_dir_all(path.parent().expect("Projects capability path has parent"))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let text = toml::to_string_pretty(observed)
        .map_err(|error| format!("could not serialize GitHub Projects capabilities: {error}"))?;
    fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn load_observed_project(
    root: impl AsRef<Path>,
    remote_name: &str,
    canonical_id: &str,
) -> Result<Option<ObservedProject>, String> {
    let path = observed_project_path(root.as_ref(), remote_name, canonical_id);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let observed: ObservedProject =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if observed.schema != OBSERVED_PROJECT_SCHEMA_V0 || observed.canonical_id != canonical_id {
        return Err(format!(
            "{}: observed ProjectV2 schema/canonical identity mismatch",
            path.display()
        ));
    }
    Ok(Some(observed))
}

pub fn write_observed_project(
    root: impl AsRef<Path>,
    remote_name: &str,
    observed: &ObservedProject,
) -> Result<(), String> {
    if observed.schema != OBSERVED_PROJECT_SCHEMA_V0 {
        return Err(format!(
            "refusing to write unsupported observed GitHub Project schema {:?}",
            observed.schema
        ));
    }
    let path = observed_project_path(root.as_ref(), remote_name, &observed.canonical_id);
    fs::create_dir_all(path.parent().expect("observed Project path has parent"))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let text = toml::to_string_pretty(observed)
        .map_err(|error| format!("could not serialize observed GitHub Project: {error}"))?;
    fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn load_observed_project_content(
    root: impl AsRef<Path>,
    remote_name: &str,
    canonical_id: &str,
) -> Result<Option<ObservedProjectContentIdentity>, String> {
    let path = observed_project_content_path(root.as_ref(), remote_name, canonical_id);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let observed: ObservedProjectContentIdentity =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if observed.schema != OBSERVED_PROJECT_CONTENT_SCHEMA_V0
        || observed.canonical_id != canonical_id
    {
        return Err(format!(
            "{}: observed Project content schema/canonical identity mismatch",
            path.display()
        ));
    }
    Ok(Some(observed))
}

pub fn write_observed_project_content(
    root: impl AsRef<Path>,
    remote_name: &str,
    observed: &ObservedProjectContentIdentity,
) -> Result<(), String> {
    if observed.schema != OBSERVED_PROJECT_CONTENT_SCHEMA_V0 {
        return Err(format!(
            "refusing to write unsupported observed GitHub Project content schema {:?}",
            observed.schema
        ));
    }
    let path = observed_project_content_path(root.as_ref(), remote_name, &observed.canonical_id);
    fs::create_dir_all(path.parent().expect("Project content path has parent"))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let text = toml::to_string_pretty(observed)
        .map_err(|error| format!("could not serialize Project content identity: {error}"))?;
    fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))
}

fn load_issue_mappings(root: &Path, remote_name: &str) -> Result<IssueMappings, String> {
    let path = root
        .join(".project/remotes")
        .join(remote_name)
        .join("mappings/issues.toml");
    if !path.exists() {
        return Ok(IssueMappings {
            schema: ISSUE_MAPPINGS_SCHEMA_V0.into(),
            issues: BTreeMap::new(),
        });
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mappings: IssueMappings =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if mappings.schema != ISSUE_MAPPINGS_SCHEMA_V0 {
        return Err(format!(
            "{}: unsupported issue mapping schema",
            path.display()
        ));
    }
    Ok(mappings)
}

fn load_review_mappings(root: &Path, remote_name: &str) -> Result<ReviewMappings, String> {
    let path = root
        .join(".project/remotes")
        .join(remote_name)
        .join("mappings/reviews.toml");
    if !path.exists() {
        return Ok(ReviewMappings {
            schema: REVIEW_MAPPINGS_SCHEMA_V0.into(),
            reviews: BTreeMap::new(),
        });
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mappings: ReviewMappings =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if mappings.schema != REVIEW_MAPPINGS_SCHEMA_V0 {
        return Err(format!(
            "{}: unsupported review mapping schema",
            path.display()
        ));
    }
    Ok(mappings)
}

fn validate_projection_config(
    path: &Path,
    config: &ProjectsProjectionConfig,
) -> Result<(), String> {
    if config.schema != PROJECTS_PROJECTION_SCHEMA_V0 {
        return Err(format!(
            "{}: unsupported GitHub Projects projection schema {:?}; expected {:?}",
            path.display(),
            config.schema,
            PROJECTS_PROJECTION_SCHEMA_V0
        ));
    }
    if !matches!(config.owner_kind.as_str(), "user" | "organization") {
        return Err(format!(
            "{}: owner_kind must be user or organization",
            path.display()
        ));
    }
    if config.owner.trim().is_empty() || config.credential_env.trim().is_empty() {
        return Err(format!(
            "{}: owner and credential_env must not be empty",
            path.display()
        ));
    }
    for (board_id, binding) in &config.boards {
        if board_id.trim().is_empty() {
            return Err(format!(
                "{}: board binding IDs must not be empty",
                path.display()
            ));
        }
        match binding.target.as_str() {
            "managed" if binding.project_number.is_some() => {
                return Err(format!(
                    "{}: managed board target {:?} must not declare project_number",
                    path.display(),
                    board_id
                ));
            }
            "existing" if binding.project_number.is_none() || binding.project_number == Some(0) => {
                return Err(format!(
                    "{}: existing board target {:?} requires a positive project_number",
                    path.display(),
                    board_id
                ));
            }
            "managed" | "existing" => {}
            _ => {
                return Err(format!(
                    "{}: board target {:?} must be managed or existing",
                    path.display(),
                    binding.target
                ));
            }
        }
    }
    Ok(())
}

fn validate_project_mappings(path: &Path, mappings: &ProjectMappings) -> Result<(), String> {
    for (board_id, mapping) in &mappings.boards {
        if board_id.trim().is_empty()
            || mapping.number == 0
            || mapping.node_id.trim().is_empty()
            || mapping.url.trim().is_empty()
            || mapping.owner_node_id.trim().is_empty()
        {
            return Err(format!(
                "{}: Project mappings require non-empty stable identities and a positive number",
                path.display()
            ));
        }
        for item in mapping.items.values() {
            if item.node_id.trim().is_empty() || item.content_node_id.trim().is_empty() {
                return Err(format!(
                    "{}: Project item mappings require node IDs",
                    path.display()
                ));
            }
        }
        for field in mapping.fields.values() {
            if field.node_id.trim().is_empty() || field.data_type.trim().is_empty() {
                return Err(format!(
                    "{}: Project field mappings require node ID and data type",
                    path.display()
                ));
            }
            if field.options.values().any(|value| value.trim().is_empty()) {
                return Err(format!(
                    "{}: Project option mappings require provider IDs",
                    path.display()
                ));
            }
        }
        if mapping
            .views
            .values()
            .any(|view| view.node_id.trim().is_empty())
        {
            return Err(format!(
                "{}: Project view mappings require node IDs",
                path.display()
            ));
        }
    }
    Ok(())
}

fn plan(remote_name: &str, repository: String, operations: Vec<GitHubOperation>) -> GitHubPlan {
    GitHubPlan {
        schema: PLAN_SCHEMA_V0.into(),
        remote: remote_name.into(),
        repository,
        operations,
    }
}

fn projects_projection_config_path(root: &Path, remote_name: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("projects.toml")
}

fn project_mappings_path(root: &Path, remote_name: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("mappings/projects.toml")
}

fn observed_projects_capabilities_path(root: &Path, remote_name: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("observed/projects/capabilities.toml")
}

fn observed_project_path(root: &Path, remote_name: &str, canonical_id: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("observed/projects/boards")
        .join(format!("{canonical_id}.toml"))
}

fn observed_project_content_path(root: &Path, remote_name: &str, canonical_id: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("observed/projects/content")
        .join(format!("{canonical_id}.toml"))
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::{IssueMapping, ReviewMapping};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn explicit_target_configuration_rejects_ambiguous_forms() {
        let root = test_root("target-config");
        write_board(&root, "issue-0001");
        write_config(&root, "managed", Some(7));
        let error = plan_boards(&root, "github").unwrap_err();
        assert!(error.contains("must not declare project_number"));
        write_config(&root, "existing", None);
        let error = plan_boards(&root, "github").unwrap_err();
        assert!(error.contains("requires a positive project_number"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn authenticated_managed_target_requires_explicit_create_runtime_when_unmapped() {
        let root = ready_root("managed-create", "managed", None);
        let plan = plan_boards(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "create_project");
        assert_eq!(plan.operations[0].fields, vec!["project:create"]);
        assert_eq!(plan.operations[0].number, None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn authenticated_existing_target_requires_numbered_bind_observation_when_unmapped() {
        let root = ready_root("existing-bind", "existing", Some(42));
        let plan = plan_boards(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].fields, vec!["project:bind-existing"]);
        assert_eq!(plan.operations[0].number, Some(42));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mapped_project_requires_observation_before_mutable_planning() {
        let root = ready_root("observe-mapped", "managed", None);
        write_project_mapping(&root, false);
        let plan = plan_boards(&root, "github").unwrap();
        assert_eq!(plan.operations[0].fields, vec!["project:observe"]);
        assert_eq!(plan.operations[0].number, Some(7));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stable_project_identity_exposes_content_identity_prerequisite() {
        let root = ready_root("content", "managed", None);
        write_project_mapping(&root, false);
        write_observed_project_fixture(&root);
        write_provider_state_fixture(&root, false);
        write_issue_mapping(&root);
        let plan = plan_boards(&root, "github").unwrap();
        assert!(
            plan.operations[0]
                .fields
                .contains(&"content-identity:issue-0001".into())
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mapping_round_trip_keeps_project_item_field_option_and_view_identity_provider_scoped() {
        let root = ready_root("mapping", "managed", None);
        write_project_mapping(&root, true);
        let mappings = load_project_mappings(&root, "github").unwrap();
        let board = mappings.boards.get("board-0001").unwrap();
        assert_eq!(board.node_id, "PVT_project");
        assert_eq!(board.items["issue-0001"].node_id, "PVTI_item");
        assert_eq!(board.fields["status"].node_id, "PVTF_field");
        assert_eq!(board.fields["status"].options["todo"], "provider-option");
        assert_eq!(board.views["development"].node_id, "PVTV_view");
        let canonical =
            fs::read_to_string(root.join(".project/boards/board-0001/board.toml")).unwrap();
        assert!(!canonical.contains("PVT_"));
        assert!(!canonical.contains("PVTI_"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn managed_board_view_creation_uses_stable_field_bridge_and_ignores_same_name_foreign_view() {
        let root = ready_root("view-create", "managed", None);
        write_project_mapping(&root, true);
        let mut mappings = load_project_mappings(&root, "github").unwrap();
        mappings.boards.get_mut("board-0001").unwrap().views.clear();
        save_project_mappings(&root, "github", &mappings).unwrap();
        write_observed_project_fixture(&root);
        write_provider_state_fixture(&root, true);
        write_issue_mapping(&root);
        write_content_identity(&root);
        let plan = plan_boards(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "create_project_view");
        assert_eq!(plan.operations[0].fields, vec!["view:development"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fully_known_and_matching_project_state_is_idempotent() {
        let root = ready_root("known", "managed", None);
        write_project_mapping(&root, true);
        write_observed_project_fixture(&root);
        write_provider_state_fixture(&root, true);
        write_issue_mapping(&root);
        write_content_identity(&root);
        let plan = plan_boards(&root, "github").unwrap();
        assert!(plan.operations.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    fn ready_root(name: &str, target: &str, project_number: Option<u64>) -> PathBuf {
        let root = test_root(name);
        write_board(&root, "issue-0001");
        write_config(&root, target, project_number);
        write_capability(&root);
        root
    }

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-github-project-identity-{name}-{}-{nonce}",
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
            "schema = \"allodium.remote/v0\"\nname = \"github\"\nkind = \"github\"\nrepository = \"sguzman/allodium\"\noutbound = \"reconcile\"\ninbound = \"archive\"\n",
        )
        .unwrap();
        root
    }

    fn write_board(root: &Path, object: &str) {
        let directory = root.join(".project/boards/board-0001");
        fs::create_dir_all(directory.join("fields")).unwrap();
        fs::create_dir_all(directory.join("items")).unwrap();
        fs::create_dir_all(directory.join("views")).unwrap();
        fs::write(
            directory.join("board.toml"),
            "schema = \"allodium.board/v0\"\nid = \"board-0001\"\ntitle = \"Board\"\ndescription = \"Test board\"\n",
        )
        .unwrap();
        fs::write(
            directory.join("fields/status.toml"),
            "schema = \"allodium.board-field/v0\"\nid = \"status\"\nname = \"Status\"\nkind = \"single_select\"\n\n[[options]]\nid = \"todo\"\nname = \"Todo\"\n",
        )
        .unwrap();
        fs::write(
            directory.join("items/issue-0001.toml"),
            format!("schema = \"allodium.board-item/v0\"\nobject = \"{object}\"\n\n[values]\nstatus = \"todo\"\n"),
        )
        .unwrap();
        fs::write(
            directory.join("views/development.toml"),
            "schema = \"allodium.board-view/v0\"\nid = \"development\"\nname = \"Development\"\nlayout = \"board\"\ngroup_by = \"status\"\n",
        )
        .unwrap();
    }

    fn write_config(root: &Path, target: &str, project_number: Option<u64>) {
        let number = project_number
            .map(|number| format!("project_number = {number}\n"))
            .unwrap_or_default();
        fs::write(
            root.join(".project/remotes/github/projects.toml"),
            format!(
                "schema = \"allodium.github.projects-projection/v0\"\nowner_kind = \"user\"\nowner = \"sguzman\"\ncredential_env = \"ALLODIUM_GITHUB_PROJECTS_TOKEN\"\n\n[boards.board-0001]\nenabled = true\ntarget = \"{target}\"\n{number}"
            ),
        )
        .unwrap();
    }

    fn write_capability(root: &Path) {
        let path = root.join(".project/remotes/github/observed/projects/capabilities.toml");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            "schema = \"allodium.github.observed-projects-capabilities/v0\"\ncredential_available = true\nauthenticated = true\nowner_kind = \"user\"\nowner = \"sguzman\"\nowner_node_id = \"U_owner\"\nobserved_at = \"2026-09-17T00:00:00Z\"\n",
        )
        .unwrap();
    }

    fn write_project_mapping(root: &Path, complete: bool) {
        let mut mapping = ProjectMapping {
            number: 7,
            node_id: "PVT_project".into(),
            url: "https://github.com/users/sguzman/projects/7".into(),
            owner_node_id: "U_owner".into(),
            items: BTreeMap::new(),
            fields: BTreeMap::new(),
            views: BTreeMap::new(),
        };
        if complete {
            mapping.items.insert(
                "issue-0001".into(),
                ProjectItemMapping {
                    node_id: "PVTI_item".into(),
                    content_node_id: "I_issue".into(),
                },
            );
            mapping.fields.insert(
                "status".into(),
                ProjectFieldMapping {
                    node_id: "PVTF_field".into(),
                    data_type: "SINGLE_SELECT".into(),
                    options: BTreeMap::from([("todo".into(), "provider-option".into())]),
                },
            );
            mapping.views.insert(
                "development".into(),
                ProjectViewMapping {
                    node_id: "PVTV_view".into(),
                },
            );
        }
        let mappings = ProjectMappings {
            schema: PROJECT_MAPPINGS_SCHEMA_V0.into(),
            boards: BTreeMap::from([("board-0001".into(), mapping)]),
        };
        save_project_mappings(root, "github", &mappings).unwrap();
    }

    fn write_observed_project_fixture(root: &Path) {
        write_observed_project(
            root,
            "github",
            &ObservedProject {
                schema: OBSERVED_PROJECT_SCHEMA_V0.into(),
                canonical_id: "board-0001".into(),
                number: 7,
                node_id: "PVT_project".into(),
                url: "https://github.com/users/sguzman/projects/7".into(),
                owner_node_id: "U_owner".into(),
                title: "Board".into(),
                short_description: "Test board".into(),
                closed: false,
                remote_updated_at: "2026-09-17T00:00:00Z".into(),
                observed_at: "2026-09-17T00:00:00Z".into(),
            },
        )
        .unwrap();
    }

    fn write_provider_state_fixture(root: &Path, complete: bool) {
        use crate::github_project_observation::{
            OBSERVED_PROJECT_PROVIDER_STATE_SCHEMA_V0, ObservedProjectProviderState,
            ObservedProviderContent, ObservedProviderField, ObservedProviderFieldValue,
            ObservedProviderItem, ObservedProviderOption, ObservedProviderView,
            write_observed_project_provider_state,
        };

        let fields = if complete {
            vec![ObservedProviderField {
                node_id: "PVTF_field".into(),
                provider_type: "ProjectV2SingleSelectField".into(),
                name: "Status".into(),
                data_type: "SINGLE_SELECT".into(),
                provider_database_id: Some(101),
                remote_updated_at: "2026-09-17T00:00:00Z".into(),
                options: vec![ObservedProviderOption {
                    id: "provider-option".into(),
                    name: "Todo".into(),
                    description: "allodium:status:active".into(),
                    color: "BLUE".into(),
                }],
                iterations: Vec::new(),
            }]
        } else {
            Vec::new()
        };
        let items = if complete {
            vec![ObservedProviderItem {
                node_id: "PVTI_item".into(),
                item_type: "ISSUE".into(),
                remote_updated_at: "2026-09-17T00:00:00Z".into(),
                content: Some(ObservedProviderContent {
                    provider_type: "Issue".into(),
                    node_id: "I_issue".into(),
                    number: Some(1),
                    repository: Some("sguzman/allodium".into()),
                    title: "Test".into(),
                    url: Some("https://github.com/sguzman/allodium/issues/1".into()),
                    body: None,
                }),
                values: vec![ObservedProviderFieldValue {
                    provider_type: "ProjectV2ItemFieldSingleSelectValue".into(),
                    field_node_id: "PVTF_field".into(),
                    field_name: "Status".into(),
                    value: "provider-option".into(),
                    remote_updated_at: "2026-09-17T00:00:00Z".into(),
                }],
            }]
        } else {
            Vec::new()
        };
        let views = if complete {
            vec![ObservedProviderView {
                node_id: "PVTV_view".into(),
                number: 1,
                full_database_id: Some("201".into()),
                name: "Development".into(),
                layout: "BOARD_LAYOUT".into(),
                filter: None,
                remote_updated_at: "2026-09-17T00:00:00Z".into(),
                visible_field_node_ids: vec!["PVTF_field".into()],
                group_by_field_node_ids: Vec::new(),
                vertical_group_by_field_node_ids: vec!["PVTF_field".into()],
                sort_by: Vec::new(),
            }]
        } else {
            Vec::new()
        };
        write_observed_project_provider_state(
            root,
            "github",
            &ObservedProjectProviderState {
                schema: OBSERVED_PROJECT_PROVIDER_STATE_SCHEMA_V0.into(),
                canonical_id: "board-0001".into(),
                number: 7,
                node_id: "PVT_project".into(),
                url: "https://github.com/users/sguzman/projects/7".into(),
                owner_node_id: "U_owner".into(),
                title: "Board".into(),
                short_description: "Test board".into(),
                closed: false,
                remote_updated_at: "2026-09-17T00:00:00Z".into(),
                observed_at: "2026-09-17T00:00:00Z".into(),
                fields,
                items,
                views,
            },
        )
        .unwrap();
    }

    fn write_issue_mapping(root: &Path) {
        let mappings = IssueMappings {
            schema: ISSUE_MAPPINGS_SCHEMA_V0.into(),
            issues: BTreeMap::from([(
                "issue-0001".into(),
                IssueMapping {
                    number: 1,
                    url: "https://github.com/sguzman/allodium/issues/1".into(),
                },
            )]),
        };
        let path = root.join(".project/remotes/github/mappings/issues.toml");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, toml::to_string_pretty(&mappings).unwrap()).unwrap();
    }

    #[allow(dead_code)]
    fn write_review_mapping(root: &Path) {
        let mappings = ReviewMappings {
            schema: REVIEW_MAPPINGS_SCHEMA_V0.into(),
            reviews: BTreeMap::from([(
                "review-0001".into(),
                ReviewMapping {
                    number: 2,
                    url: "https://github.com/sguzman/allodium/pull/2".into(),
                },
            )]),
        };
        let path = root.join(".project/remotes/github/mappings/reviews.toml");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, toml::to_string_pretty(&mappings).unwrap()).unwrap();
    }

    fn write_content_identity(root: &Path) {
        write_observed_project_content(
            root,
            "github",
            &ObservedProjectContentIdentity {
                schema: OBSERVED_PROJECT_CONTENT_SCHEMA_V0.into(),
                canonical_id: "issue-0001".into(),
                remote_type: "issue".into(),
                number: 1,
                node_id: "I_issue".into(),
                observed_at: "2026-09-17T00:00:00Z".into(),
            },
        )
        .unwrap();
    }
}
