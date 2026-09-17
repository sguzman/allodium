use super::{GitHubAdapter, project};
use allodium_core::board::{CanonicalBoard, CanonicalBoardField, CanonicalBoardItem, load_boards};
use allodium_core::github::GitHubOperation;
use allodium_core::github_project::{
    ProjectFieldMapping, ProjectItemMapping, ProjectMapping, ProjectsProjectionConfig,
    load_observed_project_content, load_observed_projects_capabilities, load_project_mappings,
    load_projects_projection_config, save_project_mappings,
};
use allodium_core::github_project_observation::load_observed_project_provider_state;
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
        "update_project_field_value" => update_field_value(adapter, root, remote_name, operation),
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
        return Err(
            "stale ProjectV2 create plan: board already has a stable provider mapping".into(),
        );
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
        return Err(
            "created ProjectV2 owner identity disagrees with authenticated configured owner".into(),
        );
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
        return Err(
            "stale ProjectV2 bind plan: operation number differs from configured target".into(),
        );
    }
    let mut mappings = load_project_mappings(root, remote_name)?;
    if mappings.boards.contains_key(&board.record.id) {
        return Err(
            "stale ProjectV2 bind plan: board already has a stable provider mapping".into(),
        );
    }
    let provider =
        project::fetch_project_by_number(adapter, &config, &token, configured_number)?
            .ok_or_else(|| "configured existing ProjectV2 target was not found".to_string())?;
    let snapshot = provider.into_snapshot(&board.record.id, "binding")?;
    if snapshot.owner_node_id != owner_node_id {
        return Err(
            "existing ProjectV2 owner identity disagrees with authenticated configured owner"
                .into(),
        );
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
            .ok_or_else(|| {
                "created single-select ProjectV2 field returned no options".to_string()
            })?;
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
    let project_mapping =
        require_fresh_project(adapter, root, remote_name, &token, &board.record.id)?;
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
    let project_mapping =
        require_fresh_project(adapter, root, remote_name, &token, &board.record.id)?;
    let item_mapping = project_mapping
        .items
        .get(canonical_item)
        .ok_or_else(|| "ProjectV2 item mapping is missing at field-value apply time".to_string())?;
    let field_mapping = project_mapping.fields.get(field_id).ok_or_else(|| {
        "ProjectV2 field mapping is missing at field-value apply time".to_string()
    })?;
    let canonical_value = item.record.values.get(field_id).ok_or_else(|| {
        "field-value plan points at a canonical value that no longer exists".to_string()
    })?;
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
    let live_owner = project::probe_projects_owner(adapter, &config, &token)?.ok_or_else(|| {
        "GitHub Projects credential failed live owner authorization probe".to_string()
    })?;
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
    let observed =
        load_observed_project_provider_state(root, remote_name, board_id)?.ok_or_else(|| {
            format!("ProjectV2 provider-state observation for {board_id:?} is missing")
        })?;
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
        "text" => Ok(
            json!({"text": value.as_str().ok_or_else(|| "text board value is not a string".to_string())?}),
        ),
        "date" => Ok(
            json!({"date": value.as_str().ok_or_else(|| "date board value is not a string".to_string())?}),
        ),
        "number" => {
            let number = if let Some(integer) = value.as_integer() {
                if integer.unsigned_abs() > 9_007_199_254_740_991u64 {
                    return Err(
                        "canonical integer board value exceeds exact GitHub GraphQL Float range"
                            .into(),
                    );
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

fn board_field<'a>(
    board: &'a CanonicalBoard,
    field_id: &str,
) -> Result<&'a CanonicalBoardField, String> {
    board
        .fields
        .iter()
        .find(|field| field.record.id == field_id)
        .ok_or_else(|| format!("canonical board field {field_id:?} no longer exists"))
}

fn board_item<'a>(
    board: &'a CanonicalBoard,
    canonical_id: &str,
) -> Result<&'a CanonicalBoardItem, String> {
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
        return Err(format!(
            "ProjectV2 operation has multiple {prefix:?} identity tags"
        ));
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
            json_response(
                200,
                r#"{"data":{"user":{"id":"U_owner","projectsV2":{"totalCount":0}}}}"#,
            ),
            json_response(
                200,
                r#"{"data":{"createProjectV2":{"projectV2":{"id":"PVT_new","number":9,"url":"https://github.com/users/sguzman/projects/9","owner":{"id":"U_owner"}}}}}"#,
            ),
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
        assert!(
            requests
                .iter()
                .all(|request| request.contains("authorization: Bearer projects-token"))
        );
        assert!(
            !requests
                .iter()
                .any(|request| request.contains("repo-token"))
        );
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
            json_response(
                200,
                r#"{"data":{"user":{"id":"U_owner","projectsV2":{"totalCount":1}}}}"#,
            ),
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
