from pathlib import Path

# Core planner: create only losslessly representable managed views and never bind by name.
core = Path("crates/allodium-core/src/github_project.rs")
text = core.read_text()
old = '''    for view in &board.views {
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
'''
new = '''    for view in &board.views {
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
                    format!("board view {:?} grouping field has no stable provider field mapping", view.record.id)
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
                format!("board view {:?} grouping field has no stable provider field mapping", view.record.id)
            })?;
            if provider_view.vertical_group_by_field_node_ids
                != vec![field_mapping.node_id.clone()]
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
'''
if old not in text:
    raise SystemExit("planner view boundary anchor not found")
text = text.replace(old, new, 1)

# Add planner proof: same-name foreign view never becomes identity; missing mapping creates explicitly.
anchor = '''    #[test]
    fn fully_known_and_matching_project_state_is_idempotent() {
'''
test = '''    #[test]
    fn managed_board_view_creation_uses_stable_field_bridge_and_ignores_same_name_foreign_view() {
        let root = ready_root("view-create", "managed", None);
        write_project_mapping(&root, true);
        let mut mappings = load_project_mappings(&root, "github").unwrap();
        mappings
            .boards
            .get_mut("board-0001")
            .unwrap()
            .views
            .clear();
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

'''
if anchor not in text:
    raise SystemExit("planner test anchor not found")
text = text.replace(anchor, test + anchor, 1)
core.write_text(text)

# Runtime: create managed table/board views exactly once through REST while retaining GraphQL node IDs.
runtime = Path("crates/allodium-github/src/project_runtime.rs")
text = runtime.read_text()
text = text.replace(
    'use allodium_core::board::{CanonicalBoard, CanonicalBoardField, CanonicalBoardItem, load_boards};',
    'use allodium_core::board::{\n    CanonicalBoard, CanonicalBoardField, CanonicalBoardItem, CanonicalBoardView, load_boards,\n};',
    1,
)
text = text.replace(
    '    ProjectFieldMapping, ProjectItemMapping, ProjectMapping, ProjectsProjectionConfig,',
    '    ProjectFieldMapping, ProjectItemMapping, ProjectMapping, ProjectViewMapping,\n    ProjectsProjectionConfig,',
    1,
)
text = text.replace(
    '    FieldValueUpdated,\n}',
    '    FieldValueUpdated,\n    ViewCreated,\n}',
    1,
)
text = text.replace(
    '        "update_project_field_value" => update_field_value(adapter, root, remote_name, operation),\n',
    '        "update_project_field_value" => update_field_value(adapter, root, remote_name, operation),\n        "create_project_view" => create_view(adapter, root, remote_name, operation),\n',
    1,
)
insert_anchor = '''fn authorized_context(
'''
create_view = r'''fn create_view(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    operation: &GitHubOperation,
) -> Result<ProjectsApplyOutcome, String> {
    let (config, token, live_owner_node_id) = authorized_context(adapter, root, remote_name)?;
    let board = board(root, &operation.canonical_id)?;
    if binding(&config, &board.record.id)?.target != "managed" {
        return Err("automatic ProjectV2 view creation is restricted to managed targets".into());
    }
    let view_id = tagged(operation, "view:")?;
    let view = board_view(&board, view_id)?;
    if view.record.layout == "roadmap" {
        return Err(
            "canonical roadmap view semantics are not losslessly representable by the audited GitHub Project view creation API"
                .into(),
        );
    }
    if !matches!(view.record.layout.as_str(), "table" | "board") {
        return Err(format!(
            "unsupported canonical ProjectV2 view layout {:?}",
            view.record.layout
        ));
    }

    let project_mapping =
        require_fresh_project(adapter, root, remote_name, &token, &board.record.id)?;
    let observed = load_observed_project_provider_state(root, remote_name, &board.record.id)?
        .ok_or_else(|| "ProjectV2 provider-state observation disappeared after freshness check".to_string())?;
    let mut mappings = load_project_mappings(root, remote_name)?;
    let board_mapping = mappings
        .boards
        .get_mut(&board.record.id)
        .ok_or_else(|| "ProjectV2 mapping disappeared after freshness check".to_string())?;
    if board_mapping.views.contains_key(view_id) {
        return Err("stale create_project_view plan: canonical view is already mapped".into());
    }

    let mut body = Map::new();
    body.insert("name".into(), Value::String(view.record.name.clone()));
    body.insert("layout".into(), Value::String(view.record.layout.clone()));
    let expected_vertical_group = if view.record.layout == "board" {
        let field_id = view
            .record
            .group_by
            .as_deref()
            .ok_or_else(|| "canonical board view is missing group_by".to_string())?;
        let field_mapping = project_mapping.fields.get(field_id).ok_or_else(|| {
            format!("canonical board view grouping field {field_id:?} has no stable provider mapping")
        })?;
        let provider_field = observed
            .fields
            .iter()
            .find(|field| field.node_id == field_mapping.node_id)
            .ok_or_else(|| {
                format!("mapped grouping field {field_id:?} is absent from provider-state observation")
            })?;
        let database_id = provider_field.provider_database_id.ok_or_else(|| {
            format!("mapped grouping field {field_id:?} has no observed provider database ID")
        })?;
        if database_id <= 0 {
            return Err(format!(
                "mapped grouping field {field_id:?} has non-positive provider database ID"
            ));
        }
        let database_id = database_id as u64;
        body.insert(
            "vertical_group_by".into(),
            Value::Array(vec![Value::Number(database_id.into())]),
        );
        Some(database_id)
    } else {
        None
    };

    let path = match config.owner_kind.as_str() {
        "organization" => format!(
            "/orgs/{}/projectsV2/{}/views",
            config.owner, project_mapping.number
        ),
        "user" => {
            let response = adapter
                .client
                .get(adapter.api_url(&format!("/users/{}", config.owner)))
                .bearer_auth(&token)
                .header("X-GitHub-Api-Version", "2026-03-10")
                .send()
                .map_err(|error| format!("GitHub user identity bridge request failed: {error}"))?;
            let user = projects_rest_json(response, "GitHub user identity bridge")?;
            let database_id = required_u64(&user, "id")?;
            let node_id = required_str(&user, "node_id")?;
            if node_id != live_owner_node_id {
                return Err(
                    "GitHub REST user identity disagrees with the live authorized Projects owner node"
                        .into(),
                );
            }
            format!(
                "/users/{database_id}/projectsV2/{}/views",
                project_mapping.number
            )
        }
        other => return Err(format!("unsupported GitHub Projects owner kind {other:?}")),
    };

    let response = adapter
        .client
        .post(adapter.api_url(&path))
        .bearer_auth(&token)
        .header("X-GitHub-Api-Version", "2026-03-10")
        .json(&Value::Object(body))
        .send()
        .map_err(|error| format!("GitHub Project view creation request failed: {error}"))?;
    let payload = projects_rest_json(response, "GitHub Project view creation")?;
    let created = payload.get("value").unwrap_or(&payload);
    let node_id = required_str(created, "node_id")?;
    if required_str(created, "name")? != view.record.name
        || required_str(created, "layout")? != view.record.layout
    {
        return Err("created GitHub Project view returned unexpected name or layout".into());
    }
    if let Some(expected) = expected_vertical_group {
        let actual = created
            .get("vertical_group_by")
            .and_then(Value::as_array)
            .ok_or_else(|| "created GitHub board view returned no vertical_group_by".to_string())?;
        if actual.len() != 1 || actual[0].as_u64() != Some(expected) {
            return Err(
                "created GitHub board view returned unexpected vertical grouping identity".into(),
            );
        }
    }

    board_mapping
        .views
        .insert(view.record.id.clone(), ProjectViewMapping { node_id });
    save_project_mappings(root, remote_name, &mappings)?;
    Ok(ProjectsApplyOutcome::ViewCreated)
}

fn projects_rest_json(
    response: reqwest::blocking::Response,
    context: &str,
) -> Result<Value, String> {
    let status = response.status();
    let body = response
        .text()
        .map_err(|error| format!("{context} response body could not be read: {error}"))?;
    if !status.is_success() {
        return Err(format!("{context} returned {status}: {body}"));
    }
    serde_json::from_str(&body).map_err(|error| format!("{context} returned invalid JSON: {error}"))
}

'''
if insert_anchor not in text:
    raise SystemExit("runtime authorized_context anchor not found")
text = text.replace(insert_anchor, create_view + insert_anchor, 1)

board_item_anchor = '''fn board_item<'a>(
'''
board_view = '''fn board_view<'a>(
    board: &'a CanonicalBoard,
    view_id: &str,
) -> Result<&'a CanonicalBoardView, String> {
    board
        .views
        .iter()
        .find(|view| view.record.id == view_id)
        .ok_or_else(|| format!("canonical board view {view_id:?} no longer exists"))
}

'''
if board_item_anchor not in text:
    raise SystemExit("runtime board item anchor not found")
text = text.replace(board_item_anchor, board_view + board_item_anchor, 1)

# Runtime fake-provider proof of REST bridge and separate credential use.
test_anchor = '''    #[test]
    fn stale_project_state_refuses_update_before_mutation() {
'''
view_test = r'''    #[test]
    fn managed_board_view_uses_verified_rest_identity_bridge_and_vertical_grouping() {
        use allodium_core::github_project_observation::{ObservedProviderField, ObservedProviderOption};

        let root = test_root("view-create");
        write_ready_root(&root, "ALLODIUM_RUNTIME_VIEW_CREATE");
        fs::write(
            root.join(".project/boards/board-0001/fields/status.toml"),
            "schema = \"allodium.board-field/v0\"\nid = \"status\"\nname = \"Status\"\nkind = \"single_select\"\n\n[[options]]\nid = \"active\"\nname = \"Active\"\n",
        )
        .unwrap();
        fs::write(
            root.join(".project/boards/board-0001/views/development.toml"),
            "schema = \"allodium.board-view/v0\"\nid = \"development\"\nname = \"Development\"\nlayout = \"board\"\ngroup_by = \"status\"\n",
        )
        .unwrap();
        let mut boards = BTreeMap::new();
        boards.insert(
            "board-0001".into(),
            ProjectMapping {
                number: 7,
                node_id: "PVT_project".into(),
                url: "https://github.com/users/sguzman/projects/7".into(),
                owner_node_id: "U_owner".into(),
                items: BTreeMap::new(),
                fields: BTreeMap::from([(
                    "status".into(),
                    ProjectFieldMapping {
                        node_id: "PVTSSF_status".into(),
                        data_type: "SINGLE_SELECT".into(),
                        options: BTreeMap::from([("active".into(), "option-active".into())]),
                    },
                )]),
                views: BTreeMap::new(),
            },
        );
        save_project_mappings(
            &root,
            "github",
            &ProjectMappings {
                schema: PROJECT_MAPPINGS_SCHEMA_V0.into(),
                boards,
            },
        )
        .unwrap();
        write_observed_project_provider_state(
            &root,
            "github",
            &ObservedProjectProviderState {
                schema: OBSERVED_PROJECT_PROVIDER_STATE_SCHEMA_V0.into(),
                canonical_id: "board-0001".into(),
                number: 7,
                node_id: "PVT_project".into(),
                url: "https://github.com/users/sguzman/projects/7".into(),
                owner_node_id: "U_owner".into(),
                title: "Board".into(),
                short_description: String::new(),
                closed: false,
                remote_updated_at: "2026-09-17T20:00:00Z".into(),
                observed_at: "2026-09-17T20:00:30Z".into(),
                fields: vec![ObservedProviderField {
                    node_id: "PVTSSF_status".into(),
                    provider_type: "ProjectV2SingleSelectField".into(),
                    name: "Status".into(),
                    data_type: "SINGLE_SELECT".into(),
                    provider_database_id: Some(101),
                    remote_updated_at: "2026-09-17T20:00:00Z".into(),
                    options: vec![ObservedProviderOption {
                        id: "option-active".into(),
                        name: "Active".into(),
                        description: "allodium:status:active".into(),
                        color: "BLUE".into(),
                    }],
                    iterations: Vec::new(),
                }],
                items: Vec::new(),
                views: Vec::new(),
            },
        )
        .unwrap();
        unsafe { env::set_var("ALLODIUM_RUNTIME_VIEW_CREATE", "projects-token") };
        let project = r#"{"data":{"node":{"id":"PVT_project","number":7,"url":"https://github.com/users/sguzman/projects/7","title":"Board","shortDescription":"","closed":false,"updatedAt":"2026-09-17T20:00:00Z","owner":{"id":"U_owner"},"fields":{"nodes":[{"__typename":"ProjectV2SingleSelectField","id":"PVTSSF_status","databaseId":101,"name":"Status","dataType":"SINGLE_SELECT","updatedAt":"2026-09-17T20:00:00Z","options":[{"id":"option-active","name":"Active","description":"allodium:status:active","color":"BLUE"}]}],"pageInfo":{"hasNextPage":false}},"views":{"nodes":[],"pageInfo":{"hasNextPage":false}},"items":{"nodes":[],"pageInfo":{"hasNextPage":false}}}}}}"#;
        let responses = vec![
            json_response(
                200,
                r#"{"data":{"user":{"id":"U_owner","projectsV2":{"totalCount":1}}}}"#,
            ),
            json_response(200, project),
            json_response(200, r#"{"id":6679733,"node_id":"U_owner","login":"sguzman"}"#),
            json_response(
                201,
                r#"{"value":{"id":201,"number":2,"node_id":"PVTV_development","name":"Development","layout":"board","vertical_group_by":[101]}}"#,
            ),
        ];
        let (base, requests, handle) = response_server(responses);
        let operation = GitHubOperation {
            canonical_id: "board-0001".into(),
            action: "create_project_view".into(),
            number: Some(7),
            fields: vec!["view:development".into()],
            reason: "test".into(),
        };
        assert_eq!(
            apply_operation(&test_adapter(base), &root, "github", &operation).unwrap(),
            ProjectsApplyOutcome::ViewCreated
        );
        handle.join().unwrap();
        unsafe { env::remove_var("ALLODIUM_RUNTIME_VIEW_CREATE") };
        let mappings = load_project_mappings(&root, "github").unwrap();
        assert_eq!(
            mappings.boards["board-0001"].views["development"].node_id,
            "PVTV_development"
        );
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 4);
        assert!(requests[2].starts_with("GET /users/sguzman "));
        assert!(requests[3].starts_with("POST /users/6679733/projectsV2/7/views "));
        assert!(requests[3].contains("authorization: Bearer projects-token"));
        assert!(requests[3].contains("\"vertical_group_by\":[101]"));
        assert!(!requests[3].contains("repo-token"));
        fs::remove_dir_all(root).unwrap();
    }

'''
if test_anchor not in text:
    raise SystemExit("runtime stale test anchor not found")
text = text.replace(test_anchor, view_test + test_anchor, 1)
runtime.write_text(text)

# Integrate the new runtime action and report it explicitly.
lib = Path("crates/allodium-github/src/lib.rs")
text = lib.read_text()
text = text.replace(
    '    pub project_field_values_updated: usize,\n',
    '    pub project_field_values_updated: usize,\n    pub project_views_created: usize,\n',
    1,
)
text = text.replace(
    '                | "update_project_field_value" => {\n',
    '                | "update_project_field_value"\n                | "create_project_view" => {\n',
    1,
)
text = text.replace(
    '''                        project_runtime::ProjectsApplyOutcome::FieldValueUpdated => {
                            report.project_field_values_updated += 1
                        }
''',
    '''                        project_runtime::ProjectsApplyOutcome::FieldValueUpdated => {
                            report.project_field_values_updated += 1
                        }
                        project_runtime::ProjectsApplyOutcome::ViewCreated => {
                            report.project_views_created += 1
                        }
''',
    1,
)
lib.write_text(text)

cli = Path("crates/allodium-cli/src/main.rs")
text = cli.read_text()
anchor = '''            println!(
                "updated {} GitHub ProjectV2 field value(s)",
                report.project_field_values_updated
            );
'''
addition = anchor + '''            println!(
                "created {} GitHub ProjectV2 view(s)",
                report.project_views_created
            );
'''
if anchor not in text:
    raise SystemExit("CLI Project field-value report anchor not found")
text = text.replace(anchor, addition, 1)
cli.write_text(text)

# Provider docs: immutable-after-create v0 boundary and explicit unsupported surfaces.
docs = Path("docs/github-projects-provider-v0.md")
text = docs.read_text()
append = '''

## Managed view creation boundary

GitHub Project views are projected conservatively in v0. Allodium may create a missing view only for an explicitly `managed` Project and only when the canonical semantics have an audited lossless creation representation. View creation is one mutation in one plan and is followed by normal re-observation before any later Project work.

Canonical `table` views can be created directly. Canonical `board` views map their `group_by` field to GitHub's board-column `vertical_group_by` configuration. The runtime starts from the stable canonical-to-provider field node mapping, finds the matching observed provider database field ID, and uses that numeric ID only as the REST API bridge demanded by GitHub. A same-named provider field or view is never identity.

User-owned Project view creation requires a numeric REST user identifier even though Allodium's stable authority identity is the GraphQL node ID. The runtime therefore resolves the configured user immediately before creation and refuses the write unless the returned REST `node_id` equals the freshly authorized Projects owner node. The separate Projects credential is used for both that bridge lookup and view creation; repository `GITHUB_TOKEN` authority is not substituted. GitHub currently documents that the user-owned view endpoint does not accept GitHub App or fine-grained PAT token classes, so an incompatible separately supplied credential fails at the provider boundary rather than causing authority fallback.

V0 treats managed view configuration as immutable after creation. Observation still archives provider drift, but Allodium does not attempt to repair name/layout/grouping changes because GitHub's currently audited write surfaces do not expose the complete configuration symmetrically. Existing-target views likewise require an explicit stable binding design and are never adopted by matching a name.

Canonical `roadmap` views remain deferred. Their `start_field`/`end_field` semantics do not have an audited lossless input in the current create-view contract, so Allodium will not create a superficially similar roadmap and call it projected. View deletion is also unsupported; canonical absence has no provider-deletion meaning in v0.
'''
if "## Managed view creation boundary" not in text:
    text += append
docs.write_text(text)
