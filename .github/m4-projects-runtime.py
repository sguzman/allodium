from pathlib import Path


def replace_between(text: str, start: str, end: str, replacement: str) -> str:
    a = text.index(start)
    b = text.index(end, a)
    return text[:a] + replacement + text[b:]

# --- deterministic planner -------------------------------------------------
core = Path("crates/allodium-core/src/github_project.rs")
text = core.read_text()
if "use crate::github_project_observation::load_observed_project_provider_state;" not in text:
    anchor = "use crate::load_remote;\n"
    text = text.replace(
        anchor,
        "use crate::github_project_observation::load_observed_project_provider_state;\n" + anchor,
        1,
    )

new_plan_board = r'''#[allow(clippy::too_many_arguments)]
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
        if !mapping.views.contains_key(&view.record.id) {
            operations.push(runtime_requirement(
                &board.record.id,
                Some(mapping.number),
                vec![format!("view:{}", view.record.id)],
                "ProjectV2 view creation/binding remains outside the first non-destructive mutation slice",
            ));
            return Ok(());
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
            let option = value
                .as_str()
                .ok_or_else(|| "canonical single-select value is not a string option id".to_string())?;
            mapping
                .options
                .get(option)
                .cloned()
                .ok_or_else(|| format!("canonical option {option:?} has no provider option mapping"))
        }
        other => Err(format!("unsupported canonical board field kind {other:?}")),
    }
}

'''
text = replace_between(text, "#[allow(clippy::too_many_arguments)]\nfn plan_board(", "fn provider_object(", new_plan_board)
text = text.replace(
    'assert_eq!(plan.operations[0].action, "projects_runtime_required");\n        assert_eq!(plan.operations[0].fields, vec!["project:create"]);',
    'assert_eq!(plan.operations[0].action, "create_project");\n        assert_eq!(plan.operations[0].fields, vec!["project:create"]);',
    1,
)
core.write_text(text)

# --- expose read-only Project helpers to sibling runtime -------------------
project = Path("crates/allodium-github/src/project.rs")
text = project.read_text()
for old, new in [
    ("fn probe_projects_owner(\n", "pub(super) fn probe_projects_owner(\n"),
    ("fn fetch_project_by_number(\n", "pub(super) fn fetch_project_by_number(\n"),
    ("fn fetch_project_by_node(\n", "pub(super) fn fetch_project_by_node(\n"),
    ("fn graphql(\n", "pub(super) fn graphql(\n"),
    ("fn graphql_errors(payload: &Value) -> Result<(), String> {", "pub(super) fn graphql_errors(payload: &Value) -> Result<(), String> {"),
    ("struct ProviderProject {", "pub(super) struct ProviderProject {"),
    ("    fn into_snapshot(\n", "    pub(super) fn into_snapshot(\n"),
]:
    if old not in text:
        raise SystemExit(f"project.rs visibility anchor missing: {old!r}")
    text = text.replace(old, new, 1)
project.write_text(text)

# --- ProjectV2 mutation runtime -------------------------------------------
runtime = Path("crates/allodium-github/src/project_runtime.rs")
runtime.write_text(r'''use super::{GitHubAdapter, project};
use allodium_core::board::{CanonicalBoard, CanonicalBoardField, CanonicalBoardItem, load_boards};
use allodium_core::github::GitHubOperation;
use allodium_core::github_project::{
    ProjectFieldMapping, ProjectItemMapping, ProjectMapping, ProjectsProjectionConfig,
    load_observed_project_content, load_observed_projects_capabilities, load_project_mappings,
    load_projects_projection_config, save_project_mappings,
};
use allodium_core::github_project_observation::{
    ObservedProjectProviderState, load_observed_project_provider_state,
};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::env;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProjectsApplyOutcome {
    Created,
    Bound,
    Observed(usize),
    Updated,
    FieldCreated,
    ItemAdded,
    FieldValueUpdated,
}

pub(super) fn apply_operation(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    operation: &GitHubOperation,
) -> Result<ProjectsApplyOutcome, String> {
    match operation.action.as_str() {
        "create_project" => create_project(adapter, root, remote_name, operation),
        "bind_project" => bind_project(adapter, root, remote_name, operation),
        "observe_project" => {
            let report = project::observe_projects(adapter, root, remote_name)?;
            Ok(ProjectsApplyOutcome::Observed(report.projects_observed))
        }
        "update_project" => update_project(adapter, root, remote_name, operation),
        "create_project_field" => create_field(adapter, root, remote_name, operation),
        "add_project_item" => add_item(adapter, root, remote_name, operation),
        "update_project_field_value" => {
            update_field_value(adapter, root, remote_name, operation)
        }
        other => Err(format!("unsupported ProjectV2 runtime operation {other:?}")),
    }
}

fn create_project(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    operation: &GitHubOperation,
) -> Result<ProjectsApplyOutcome, String> {
    let (config, token, owner_node_id) = authorized_context(adapter, root, remote_name)?;
    let board = board(root, &operation.canonical_id)?;
    let binding = binding(&config, &board.record.id)?;
    if binding.target != "managed" {
        return Err("create_project requires an explicit managed ProjectV2 target".into());
    }
    let mut mappings = load_project_mappings(root, remote_name)?;
    if mappings.boards.contains_key(&board.record.id) {
        return Err("stale ProjectV2 create plan: board already has a stable provider mapping".into());
    }

    let query = r#"
        mutation($owner: ID!, $title: String!) {
          createProjectV2(input: {ownerId: $owner, title: $title}) {
            projectV2 { id number url owner { id } }
          }
        }
    "#;
    let payload = project::graphql(
        adapter,
        &token,
        query,
        json!({"owner": owner_node_id, "title": board.record.title}),
    )?;
    project::graphql_errors(&payload)?;
    let created = payload
        .pointer("/data/createProjectV2/projectV2")
        .ok_or_else(|| "GitHub createProjectV2 returned no project".to_string())?;
    let node_id = required_str(created, "id")?;
    let number = required_u64(created, "number")?;
    let url = required_str(created, "url")?;
    let returned_owner = created
        .pointer("/owner/id")
        .and_then(Value::as_str)
        .ok_or_else(|| "GitHub createProjectV2 returned no owner identity".to_string())?;
    if returned_owner != owner_node_id {
        return Err("created ProjectV2 owner identity disagrees with authenticated configured owner".into());
    }
    mappings.boards.insert(
        board.record.id.clone(),
        ProjectMapping {
            number,
            node_id,
            url,
            owner_node_id,
            items: BTreeMap::new(),
            fields: BTreeMap::new(),
            views: BTreeMap::new(),
        },
    );
    save_project_mappings(root, remote_name, &mappings)?;
    Ok(ProjectsApplyOutcome::Created)
}

fn bind_project(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    operation: &GitHubOperation,
) -> Result<ProjectsApplyOutcome, String> {
    let (config, token, owner_node_id) = authorized_context(adapter, root, remote_name)?;
    let board = board(root, &operation.canonical_id)?;
    let binding = binding(&config, &board.record.id)?;
    if binding.target != "existing" {
        return Err("bind_project requires an explicit existing ProjectV2 target".into());
    }
    let configured_number = binding
        .project_number
        .ok_or_else(|| "existing ProjectV2 target has no configured number".to_string())?;
    if operation.number != Some(configured_number) {
        return Err("stale ProjectV2 bind plan: operation number differs from configured target".into());
    }
    let mut mappings = load_project_mappings(root, remote_name)?;
    if mappings.boards.contains_key(&board.record.id) {
        return Err("stale ProjectV2 bind plan: board already has a stable provider mapping".into());
    }
    let provider = project::fetch_project_by_number(adapter, &config, &token, configured_number)?
        .ok_or_else(|| "configured existing ProjectV2 target was not found".to_string())?;
    let snapshot = provider.into_snapshot(&board.record.id, "binding")?;
    if snapshot.owner_node_id != owner_node_id {
        return Err("existing ProjectV2 owner identity disagrees with authenticated configured owner".into());
    }
    mappings.boards.insert(
        board.record.id.clone(),
        ProjectMapping {
            number: snapshot.number,
            node_id: snapshot.node_id,
            url: snapshot.url,
            owner_node_id: snapshot.owner_node_id,
            items: BTreeMap::new(),
            fields: BTreeMap::new(),
            views: BTreeMap::new(),
        },
    );
    save_project_mappings(root, remote_name, &mappings)?;
    Ok(ProjectsApplyOutcome::Bound)
}

fn update_project(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    operation: &GitHubOperation,
) -> Result<ProjectsApplyOutcome, String> {
    let (_config, token, _owner_node_id) = authorized_context(adapter, root, remote_name)?;
    let board = board(root, &operation.canonical_id)?;
    let mapping = require_fresh_project(adapter, root, remote_name, &token, &board.record.id)?;

    let mut definitions = vec!["$project: ID!"];
    let mut inputs = vec!["projectId: $project"];
    let mut variables = Map::new();
    variables.insert("project".into(), Value::String(mapping.node_id.clone()));
    if operation.fields.iter().any(|field| field == "title") {
        definitions.push("$title: String");
        inputs.push("title: $title");
        variables.insert("title".into(), Value::String(board.record.title.clone()));
    }
    if operation
        .fields
        .iter()
        .any(|field| field == "short_description")
    {
        definitions.push("$short: String");
        inputs.push("shortDescription: $short");
        variables.insert(
            "short".into(),
            Value::String(board.record.description.clone()),
        );
    }
    if operation.fields.iter().any(|field| field == "closed") {
        definitions.push("$closed: Boolean");
        inputs.push("closed: $closed");
        variables.insert("closed".into(), Value::Bool(false));
    }
    if inputs.len() == 1 {
        return Err("update_project plan contained no managed fields".into());
    }
    let query = format!(
        "mutation({}) {{ updateProjectV2(input: {{ {} }}) {{ projectV2 {{ id }} }} }}",
        definitions.join(", "),
        inputs.join(", ")
    );
    let payload = project::graphql(adapter, &token, &query, Value::Object(variables))?;
    project::graphql_errors(&payload)?;
    Ok(ProjectsApplyOutcome::Updated)
}

fn create_field(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    operation: &GitHubOperation,
) -> Result<ProjectsApplyOutcome, String> {
    let (config, token, _owner_node_id) = authorized_context(adapter, root, remote_name)?;
    let board = board(root, &operation.canonical_id)?;
    if binding(&config, &board.record.id)?.target != "managed" {
        return Err("automatic ProjectV2 field creation is restricted to managed targets".into());
    }
    let field_id = tagged(operation, "field:")?;
    let field = board_field(&board, field_id)?;
    let mapping = require_fresh_project(adapter, root, remote_name, &token, &board.record.id)?;
    let mut mappings = load_project_mappings(root, remote_name)?;
    let board_mapping = mappings
        .boards
        .get_mut(&board.record.id)
        .ok_or_else(|| "ProjectV2 mapping disappeared after freshness check".to_string())?;
    if board_mapping.fields.contains_key(field_id) {
        return Err("stale create_project_field plan: canonical field is already mapped".into());
    }

    let data_type = provider_data_type(&field.record.kind)?;
    let options = if field.record.kind == "single_select" {
        Value::Array(
            field
                .record
                .options
                .iter()
                .enumerate()
                .map(|(index, option)| {
                    json!({
                        "name": option.name,
                        "description": option_marker(&field.record.id, &option.id),
                        "color": option_color(index),
                    })
                })
                .collect(),
        )
    } else {
        Value::Null
    };
    let query = r#"
        mutation(
          $project: ID!,
          $name: String!,
          $dataType: ProjectV2CustomFieldType!,
          $options: [ProjectV2SingleSelectFieldOptionInput!]
        ) {
          createProjectV2Field(input: {
            projectId: $project,
            name: $name,
            dataType: $dataType,
            singleSelectOptions: $options
          }) {
            projectV2Field {
              __typename
              ... on ProjectV2Field { id name dataType }
              ... on ProjectV2SingleSelectField {
                id name dataType options { id name description }
              }
            }
          }
        }
    "#;
    let payload = project::graphql(
        adapter,
        &token,
        query,
        json!({
            "project": mapping.node_id,
            "name": format!("Allodium: {}", field.record.name),
            "dataType": data_type,
            "options": options,
        }),
    )?;
    project::graphql_errors(&payload)?;
    let created = payload
        .pointer("/data/createProjectV2Field/projectV2Field")
        .ok_or_else(|| "GitHub createProjectV2Field returned no field".to_string())?;
    let node_id = required_str(created, "id")?;
    let returned_type = required_str(created, "dataType")?;
    if returned_type != data_type {
        return Err("created ProjectV2 field returned an unexpected provider data type".into());
    }
    let mut option_mappings = BTreeMap::new();
    if field.record.kind == "single_select" {
        let returned = created
            .get("options")
            .and_then(Value::as_array)
            .ok_or_else(|| "created single-select ProjectV2 field returned no options".to_string())?;
        for canonical in &field.record.options {
            let marker = option_marker(&field.record.id, &canonical.id);
            let provider = returned
                .iter()
                .find(|option| {
                    option.get("description").and_then(Value::as_str) == Some(marker.as_str())
                })
                .ok_or_else(|| {
                    format!(
                        "created ProjectV2 option for canonical option {:?} could not be identified by its Allodium marker",
                        canonical.id
                    )
                })?;
            option_mappings.insert(canonical.id.clone(), required_str(provider, "id")?);
        }
    }
    board_mapping.fields.insert(
        field.record.id.clone(),
        ProjectFieldMapping {
            node_id,
            data_type: data_type.into(),
            options: option_mappings,
        },
    );
    save_project_mappings(root, remote_name, &mappings)?;
    Ok(ProjectsApplyOutcome::FieldCreated)
}

fn add_item(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    operation: &GitHubOperation,
) -> Result<ProjectsApplyOutcome, String> {
    let (_config, token, _owner_node_id) = authorized_context(adapter, root, remote_name)?;
    let board = board(root, &operation.canonical_id)?;
    let canonical_id = tagged(operation, "item:")?;
    board_item(&board, canonical_id)?;
    let project_mapping = require_fresh_project(adapter, root, remote_name, &token, &board.record.id)?;
    let content = load_observed_project_content(root, remote_name, canonical_id)?
        .ok_or_else(|| format!("missing ProjectV2 content identity for {canonical_id:?}"))?;
    let mut mappings = load_project_mappings(root, remote_name)?;
    let board_mapping = mappings
        .boards
        .get_mut(&board.record.id)
        .ok_or_else(|| "ProjectV2 mapping disappeared after freshness check".to_string())?;
    if board_mapping.items.contains_key(canonical_id) {
        return Err("stale add_project_item plan: canonical object is already mapped".into());
    }
    let query = r#"
        mutation($project: ID!, $content: ID!) {
          addProjectV2ItemById(input: {projectId: $project, contentId: $content}) {
            item { id }
          }
        }
    "#;
    let payload = project::graphql(
        adapter,
        &token,
        query,
        json!({"project": project_mapping.node_id, "content": content.node_id}),
    )?;
    project::graphql_errors(&payload)?;
    let item_id = payload
        .pointer("/data/addProjectV2ItemById/item/id")
        .and_then(Value::as_str)
        .ok_or_else(|| "GitHub addProjectV2ItemById returned no item id".to_string())?
        .to_owned();
    board_mapping.items.insert(
        canonical_id.into(),
        ProjectItemMapping {
            node_id: item_id,
            content_node_id: content.node_id,
        },
    );
    save_project_mappings(root, remote_name, &mappings)?;
    Ok(ProjectsApplyOutcome::ItemAdded)
}

fn update_field_value(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    operation: &GitHubOperation,
) -> Result<ProjectsApplyOutcome, String> {
    let (_config, token, _owner_node_id) = authorized_context(adapter, root, remote_name)?;
    let board = board(root, &operation.canonical_id)?;
    let canonical_item = tagged(operation, "item:")?;
    let field_id = tagged(operation, "field:")?;
    let item = board_item(&board, canonical_item)?;
    let field = board_field(&board, field_id)?;
    let project_mapping = require_fresh_project(adapter, root, remote_name, &token, &board.record.id)?;
    let item_mapping = project_mapping
        .items
        .get(canonical_item)
        .ok_or_else(|| "ProjectV2 item mapping is missing at field-value apply time".to_string())?;
    let field_mapping = project_mapping
        .fields
        .get(field_id)
        .ok_or_else(|| "ProjectV2 field mapping is missing at field-value apply time".to_string())?;
    let canonical_value = item
        .record
        .values
        .get(field_id)
        .ok_or_else(|| "field-value plan points at a canonical value that no longer exists".to_string())?;
    let value = mutation_field_value(&field.record.kind, canonical_value, field_mapping)?;
    let query = r#"
        mutation($project: ID!, $item: ID!, $field: ID!, $value: ProjectV2FieldValue!) {
          updateProjectV2ItemFieldValue(input: {
            projectId: $project,
            itemId: $item,
            fieldId: $field,
            value: $value
          }) {
            projectV2Item { id }
          }
        }
    "#;
    let payload = project::graphql(
        adapter,
        &token,
        query,
        json!({
            "project": project_mapping.node_id,
            "item": item_mapping.node_id,
            "field": field_mapping.node_id,
            "value": value,
        }),
    )?;
    project::graphql_errors(&payload)?;
    Ok(ProjectsApplyOutcome::FieldValueUpdated)
}

fn authorized_context(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
) -> Result<(ProjectsProjectionConfig, String, String), String> {
    let config = load_projects_projection_config(root, remote_name)?
        .ok_or_else(|| "GitHub Projects projection configuration is missing".to_string())?;
    let observed = load_observed_projects_capabilities(root, remote_name)?
        .ok_or_else(|| "GitHub Projects capability observation is missing".to_string())?;
    if !observed.credential_available
        || !observed.authenticated
        || observed.owner_kind != config.owner_kind
        || observed.owner != config.owner
    {
        return Err("stale or unauthorized GitHub Projects capability observation".into());
    }
    let token = env::var(&config.credential_env)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "GitHub Projects credential environment variable {} is absent",
                config.credential_env
            )
        })?;
    let live_owner = project::probe_projects_owner(adapter, &config, &token)?
        .ok_or_else(|| "GitHub Projects credential failed live owner authorization probe".to_string())?;
    if observed.owner_node_id.as_deref() != Some(live_owner.as_str()) {
        return Err("GitHub Projects owner identity changed since capability observation".into());
    }
    Ok((config, token, live_owner))
}

fn require_fresh_project(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    token: &str,
    board_id: &str,
) -> Result<ProjectMapping, String> {
    let mappings = load_project_mappings(root, remote_name)?;
    let mapping = mappings
        .boards
        .get(board_id)
        .cloned()
        .ok_or_else(|| format!("ProjectV2 mapping for {board_id:?} is missing"))?;
    let observed = load_observed_project_provider_state(root, remote_name, board_id)?
        .ok_or_else(|| format!("ProjectV2 provider-state observation for {board_id:?} is missing"))?;
    if observed.node_id != mapping.node_id || observed.number != mapping.number {
        return Err("ProjectV2 observation identity disagrees with stable mapping".into());
    }
    let live = project::fetch_project_by_node(adapter, token, &mapping.node_id)?
        .ok_or_else(|| "mapped ProjectV2 no longer exists or is no longer visible".to_string())?;
    let live = live.into_snapshot(board_id, &observed.observed_at)?;
    if live != observed {
        return Err(
            "stale GitHub ProjectV2 observation: provider state changed after planning; refusing mutable write"
                .into(),
        );
    }
    Ok(mapping)
}

fn mutation_field_value(
    kind: &str,
    value: &toml::Value,
    mapping: &ProjectFieldMapping,
) -> Result<Value, String> {
    match kind {
        "text" => Ok(json!({"text": value.as_str().ok_or_else(|| "text board value is not a string".to_string())?})),
        "date" => Ok(json!({"date": value.as_str().ok_or_else(|| "date board value is not a string".to_string())?})),
        "number" => {
            let number = if let Some(integer) = value.as_integer() {
                if integer.unsigned_abs() > 9_007_199_254_740_991u64 {
                    return Err("canonical integer board value exceeds exact GitHub GraphQL Float range".into());
                }
                integer as f64
            } else {
                value
                    .as_float()
                    .ok_or_else(|| "number board value is not numeric".to_string())?
            };
            Ok(json!({"number": number}))
        }
        "single_select" => {
            let canonical = value
                .as_str()
                .ok_or_else(|| "single-select board value is not a string option id".to_string())?;
            let provider = mapping.options.get(canonical).ok_or_else(|| {
                format!("canonical option {canonical:?} has no ProjectV2 option mapping")
            })?;
            Ok(json!({"singleSelectOptionId": provider}))
        }
        other => Err(format!("unsupported canonical board field kind {other:?}")),
    }
}

fn board(root: &Path, board_id: &str) -> Result<CanonicalBoard, String> {
    load_boards(root)?
        .into_iter()
        .find(|board| board.record.id == board_id)
        .ok_or_else(|| format!("canonical board {board_id:?} no longer exists"))
}

fn board_field<'a>(board: &'a CanonicalBoard, field_id: &str) -> Result<&'a CanonicalBoardField, String> {
    board
        .fields
        .iter()
        .find(|field| field.record.id == field_id)
        .ok_or_else(|| format!("canonical board field {field_id:?} no longer exists"))
}

fn board_item<'a>(board: &'a CanonicalBoard, canonical_id: &str) -> Result<&'a CanonicalBoardItem, String> {
    board
        .items
        .iter()
        .find(|item| item.record.object == canonical_id)
        .ok_or_else(|| format!("canonical board item {canonical_id:?} no longer exists"))
}

fn binding<'a>(
    config: &'a ProjectsProjectionConfig,
    board_id: &str,
) -> Result<&'a allodium_core::github_project::ProjectBoardBinding, String> {
    config
        .boards
        .get(board_id)
        .filter(|binding| binding.enabled)
        .ok_or_else(|| format!("canonical board {board_id:?} is not enabled for GitHub Projects"))
}

fn tagged<'a>(operation: &'a GitHubOperation, prefix: &str) -> Result<&'a str, String> {
    let mut values = operation
        .fields
        .iter()
        .filter_map(|field| field.strip_prefix(prefix));
    let value = values
        .next()
        .ok_or_else(|| format!("ProjectV2 operation is missing {prefix:?} identity tag"))?;
    if values.next().is_some() {
        return Err(format!("ProjectV2 operation has multiple {prefix:?} identity tags"));
    }
    Ok(value)
}

fn provider_data_type(kind: &str) -> Result<&'static str, String> {
    match kind {
        "text" => Ok("TEXT"),
        "number" => Ok("NUMBER"),
        "date" => Ok("DATE"),
        "single_select" => Ok("SINGLE_SELECT"),
        other => Err(format!("unsupported canonical board field kind {other:?}")),
    }
}

fn option_marker(field_id: &str, option_id: &str) -> String {
    format!("allodium:{field_id}:{option_id}")
}

fn option_color(index: usize) -> &'static str {
    const COLORS: [&str; 8] = [
        "GRAY", "BLUE", "GREEN", "YELLOW", "ORANGE", "RED", "PURPLE", "PINK",
    ];
    COLORS[index % COLORS.len()]
}

fn required_str(value: &Value, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("GitHub ProjectV2 response is missing non-empty {key:?}"))
}

fn required_u64(value: &Value, key: &str) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("GitHub ProjectV2 response is missing numeric {key:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use allodium_core::github_project::{
        OBSERVED_PROJECTS_CAPABILITIES_SCHEMA_V0, ObservedProjectsCapabilities,
        PROJECT_MAPPINGS_SCHEMA_V0, ProjectMappings, write_observed_projects_capabilities,
    };
    use allodium_core::github_project_observation::{
        OBSERVED_PROJECT_PROVIDER_STATE_SCHEMA_V0, ObservedProjectProviderState,
        write_observed_project_provider_state,
    };
    use reqwest::blocking::Client;
    use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
    use std::fs;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn managed_create_uses_separate_projects_authority_and_persists_mapping() {
        let root = test_root("create");
        write_ready_root(&root, "ALLODIUM_RUNTIME_CREATE");
        unsafe { env::set_var("ALLODIUM_RUNTIME_CREATE", "projects-token") };
        let responses = vec![
            json_response(200, r#"{"data":{"user":{"id":"U_owner","projectsV2":{"totalCount":0}}}}"#),
            json_response(200, r#"{"data":{"createProjectV2":{"projectV2":{"id":"PVT_new","number":9,"url":"https://github.com/users/sguzman/projects/9","owner":{"id":"U_owner"}}}}}"#),
        ];
        let (base, requests, handle) = response_server(responses);
        let adapter = test_adapter(base);
        let operation = GitHubOperation {
            canonical_id: "board-0001".into(),
            action: "create_project".into(),
            number: None,
            fields: vec!["project:create".into()],
            reason: "test".into(),
        };
        assert_eq!(
            apply_operation(&adapter, &root, "github", &operation).unwrap(),
            ProjectsApplyOutcome::Created
        );
        handle.join().unwrap();
        let mappings = load_project_mappings(&root, "github").unwrap();
        let mapping = mappings.boards.get("board-0001").unwrap();
        assert_eq!(mapping.node_id, "PVT_new");
        assert_eq!(mapping.number, 9);
        let requests = requests.lock().unwrap();
        assert!(requests.iter().all(|request| request.contains("authorization: Bearer projects-token")));
        assert!(!requests.iter().any(|request| request.contains("repo-token")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_project_state_refuses_update_before_mutation() {
        let root = test_root("stale");
        write_ready_root(&root, "ALLODIUM_RUNTIME_STALE");
        write_mapping(&root);
        write_observed_state(&root, "Board", "2026-09-17T20:00:00Z");
        unsafe { env::set_var("ALLODIUM_RUNTIME_STALE", "projects-token") };
        let live = project_response("Provider changed", "2026-09-17T20:01:00Z");
        let responses = vec![
            json_response(200, r#"{"data":{"user":{"id":"U_owner","projectsV2":{"totalCount":1}}}}"#),
            json_response(200, &live),
        ];
        let (base, requests, handle) = response_server(responses);
        let operation = GitHubOperation {
            canonical_id: "board-0001".into(),
            action: "update_project".into(),
            number: Some(7),
            fields: vec!["title".into()],
            reason: "test".into(),
        };
        let error = apply_operation(&test_adapter(base), &root, "github", &operation).unwrap_err();
        handle.join().unwrap();
        assert!(error.contains("stale GitHub ProjectV2 observation"));
        assert_eq!(requests.lock().unwrap().len(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn single_select_mutation_uses_provider_option_identity() {
        let mut options = BTreeMap::new();
        options.insert("active".into(), "provider-option-42".into());
        let mapping = ProjectFieldMapping {
            node_id: "PVTSSF_status".into(),
            data_type: "SINGLE_SELECT".into(),
            options,
        };
        let value = mutation_field_value(
            "single_select",
            &toml::Value::String("active".into()),
            &mapping,
        )
        .unwrap();
        assert_eq!(value, json!({"singleSelectOptionId": "provider-option-42"}));
    }

    fn write_ready_root(root: &Path, credential_env: &str) {
        fs::create_dir_all(root.join(".project/boards/board-0001/fields")).unwrap();
        fs::create_dir_all(root.join(".project/boards/board-0001/items")).unwrap();
        fs::create_dir_all(root.join(".project/boards/board-0001/views")).unwrap();
        fs::create_dir_all(root.join(".project/remotes/github")).unwrap();
        fs::write(
            root.join(".project/boards/board-0001/board.toml"),
            "schema = \"allodium.board/v0\"\nid = \"board-0001\"\ntitle = \"Board\"\ndescription = \"\"\n",
        )
        .unwrap();
        fs::write(
            root.join(".project/remotes/github/projects.toml"),
            format!(
                "schema = \"allodium.github.projects-projection/v0\"\nowner_kind = \"user\"\nowner = \"sguzman\"\ncredential_env = {credential_env:?}\n\n[boards.board-0001]\nenabled = true\ntarget = \"managed\"\n"
            ),
        )
        .unwrap();
        write_observed_projects_capabilities(
            root,
            "github",
            &ObservedProjectsCapabilities {
                schema: OBSERVED_PROJECTS_CAPABILITIES_SCHEMA_V0.into(),
                credential_available: true,
                authenticated: true,
                owner_kind: "user".into(),
                owner: "sguzman".into(),
                owner_node_id: Some("U_owner".into()),
                observed_at: "2026-09-17T19:59:00Z".into(),
            },
        )
        .unwrap();
    }

    fn write_mapping(root: &Path) {
        let mut boards = BTreeMap::new();
        boards.insert(
            "board-0001".into(),
            ProjectMapping {
                number: 7,
                node_id: "PVT_project".into(),
                url: "https://github.com/users/sguzman/projects/7".into(),
                owner_node_id: "U_owner".into(),
                items: BTreeMap::new(),
                fields: BTreeMap::new(),
                views: BTreeMap::new(),
            },
        );
        save_project_mappings(
            root,
            "github",
            &ProjectMappings {
                schema: PROJECT_MAPPINGS_SCHEMA_V0.into(),
                boards,
            },
        )
        .unwrap();
    }

    fn write_observed_state(root: &Path, title: &str, updated_at: &str) {
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
                title: title.into(),
                short_description: String::new(),
                closed: false,
                remote_updated_at: updated_at.into(),
                observed_at: "2026-09-17T20:00:30Z".into(),
                fields: Vec::new(),
                items: Vec::new(),
                views: Vec::new(),
            },
        )
        .unwrap();
    }

    fn project_response(title: &str, updated_at: &str) -> String {
        format!(
            r#"{{"data":{{"node":{{"id":"PVT_project","number":7,"url":"https://github.com/users/sguzman/projects/7","title":{title:?},"shortDescription":"","closed":false,"updatedAt":{updated_at:?},"owner":{{"id":"U_owner"}},"fields":{{"nodes":[],"pageInfo":{{"hasNextPage":false}}}},"views":{{"nodes":[],"pageInfo":{{"hasNextPage":false}}}},"items":{{"nodes":[],"pageInfo":{{"hasNextPage":false}}}}}}}}}}"#
        )
    }

    fn test_adapter(api_base: String) -> GitHubAdapter {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("allodium-test"));
        GitHubAdapter {
            client: Client::builder().default_headers(headers).build().unwrap(),
            repository: "sguzman/allodium".into(),
            token: Some("repo-token".into()),
            api_base,
        }
    }

    fn response_server(
        responses: Vec<String>,
    ) -> (String, Arc<Mutex<Vec<String>>>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let handle = thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_request(&mut stream);
                captured.lock().unwrap().push(request);
                stream.write_all(response.as_bytes()).unwrap();
                stream.flush().unwrap();
            }
        });
        (format!("http://{address}"), requests, handle)
    }

    fn read_request(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let read = stream.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);
            if let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                let header_end = header_end + 4;
                let headers = String::from_utf8_lossy(&bytes[..header_end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if bytes.len() >= header_end + length {
                    break;
                }
            }
        }
        String::from_utf8_lossy(&bytes).to_string()
    }

    fn json_response(status: u16, body: &str) -> String {
        let reason = if status == 200 { "OK" } else { "Error" };
        format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = env::temp_dir().join(format!(
            "allodium-project-runtime-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }
}
''')

# --- wire adapter apply/report --------------------------------------------
lib = Path("crates/allodium-github/src/lib.rs")
text = lib.read_text()
text = text.replace("mod project;\n", "mod project;\nmod project_runtime;\n", 1)
old_fields = """    pub projects_capabilities_observed: usize,\n    pub projects_config_required: usize,\n    pub projects_auth_required: usize,\n    pub projects_owner_review_required: usize,\n    pub projects_runtime_required: usize,\n"""
new_fields = """    pub projects_capabilities_observed: usize,\n    pub projects_created: usize,\n    pub projects_bound: usize,\n    pub projects_observed: usize,\n    pub projects_updated: usize,\n    pub project_fields_created: usize,\n    pub project_items_added: usize,\n    pub project_field_values_updated: usize,\n    pub projects_config_required: usize,\n    pub projects_auth_required: usize,\n    pub projects_owner_review_required: usize,\n    pub projects_runtime_required: usize,\n"""
if old_fields not in text:
    raise SystemExit("ApplyReport Projects field block missing")
text = text.replace(old_fields, new_fields, 1)
anchor = '''                "observe_projects_capabilities" => {
                    let projects =
                        project::observe_projects_capabilities(self, root, &plan.remote)?;
                    report.projects_capabilities_observed += projects.capabilities_observed;
                }
'''
insert = anchor + '''                "create_project"
                | "bind_project"
                | "observe_project"
                | "update_project"
                | "create_project_field"
                | "add_project_item"
                | "update_project_field_value" => {
                    match project_runtime::apply_operation(self, root, &plan.remote, operation)? {
                        project_runtime::ProjectsApplyOutcome::Created => report.projects_created += 1,
                        project_runtime::ProjectsApplyOutcome::Bound => report.projects_bound += 1,
                        project_runtime::ProjectsApplyOutcome::Observed(count) => report.projects_observed += count,
                        project_runtime::ProjectsApplyOutcome::Updated => report.projects_updated += 1,
                        project_runtime::ProjectsApplyOutcome::FieldCreated => report.project_fields_created += 1,
                        project_runtime::ProjectsApplyOutcome::ItemAdded => report.project_items_added += 1,
                        project_runtime::ProjectsApplyOutcome::FieldValueUpdated => report.project_field_values_updated += 1,
                    }
                }
'''
if anchor not in text:
    raise SystemExit("Projects apply dispatch anchor missing")
text = text.replace(anchor, insert, 1)
lib.write_text(text)

# --- CLI reporting ---------------------------------------------------------
cli = Path("crates/allodium-cli/src/main.rs")
text = cli.read_text()
anchor = '''            println!(
                "observed {} GitHub Projects capability snapshot(s)",
                report.projects_capabilities_observed
            );
            println!(
                "{} GitHub Projects projection(s) require configuration",
                report.projects_config_required
            );
'''
replacement = '''            println!(
                "observed {} GitHub Projects capability snapshot(s)",
                report.projects_capabilities_observed
            );
            println!("created {} GitHub ProjectV2 project(s)", report.projects_created);
            println!("bound {} existing GitHub ProjectV2 project(s)", report.projects_bound);
            println!("observed {} GitHub ProjectV2 project(s) during apply", report.projects_observed);
            println!("updated {} GitHub ProjectV2 project(s)", report.projects_updated);
            println!("created {} GitHub ProjectV2 field(s)", report.project_fields_created);
            println!("added {} GitHub ProjectV2 item(s)", report.project_items_added);
            println!(
                "updated {} GitHub ProjectV2 field value(s)",
                report.project_field_values_updated
            );
            println!(
                "{} GitHub Projects projection(s) require configuration",
                report.projects_config_required
            );
'''
if anchor not in text:
    raise SystemExit("CLI Projects report anchor missing")
text = text.replace(anchor, replacement, 1)
text = text.replace(
    '"{} GitHub Projects projection(s) are waiting for ProjectV2 runtime",',
    '"{} GitHub Projects projection(s) remain at a deferred/non-mutating provider boundary",',
    1,
)
cli.write_text(text)

# --- provider docs ---------------------------------------------------------
docs = Path("docs/github-projects-provider-v0.md")
text = docs.read_text()
text = text.replace(
    "## Current mutation boundary\n\nThis increment is identity/planning only. The planner can distinguish managed creation, binding an explicitly numbered existing Project, missing observation, content identity prerequisites, item/field/option/view mapping gaps, and identity disagreement. All such work still emits the non-mutating `projects_runtime_required` boundary. No ProjectV2 deletion is planned, and canonical absence has no provider deletion meaning yet.\n",
    "## Non-destructive mutation boundary\n\nThe first ProjectV2 mutation runtime is deliberately staged. Planning emits at most one mutable ProjectV2 operation per canonical board, so every project creation, provider-field creation, item-membership addition, metadata update, or field-value update is separated by a fresh observation boundary. This matches GitHub's own API contract that adding an item and updating its field values are separate mutations.\n\nManaged targets may create ProjectV2 projects and provider fields. Allodium-created provider fields are namespaced as `Allodium: <canonical name>` and immediately receive stable field/option mappings; this avoids guessing identity from GitHub field names or accidentally adopting provider defaults. Existing targets bind only by the explicitly configured Project number plus stable node identity. Missing field identity on an existing target remains a review/configuration boundary rather than a name-match heuristic.\n\nBefore every mutable write to an already-mapped Project, the runtime re-fetches the full normalized ProjectV2 state by stable node ID and compares it to the persisted provider-state observation. Any intervening provider change causes a stale-state refusal before mutation. Explicit canonical item values are managed only after both Project item and field identities exist; canonical absence does not clear provider values in v0. Project/item deletion remains unsupported, and view creation/binding remains a later non-destructive slice.\n",
    1,
)
docs.write_text(text)
