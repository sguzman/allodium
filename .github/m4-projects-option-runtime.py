from pathlib import Path

# Preserve provider option order in observation; whole-schema updates must not reorder it accidentally.
project = Path("crates/allodium-github/src/project.rs")
text = project.read_text()
text = text.replace(
'''    let mut options = value
        .get("options")
''',
'''    let options = value
        .get("options")
''',
1,
)
text = text.replace(
'''        .collect::<Result<Vec<_>, String>>()?;
    options.sort_by(|left, right| left.id.cmp(&right.id));
    let mut iterations = Vec::new();
''',
'''        .collect::<Result<Vec<_>, String>>()?;
    let mut iterations = Vec::new();
''',
1,
)
project.write_text(text)

# Prove observation preserves provider option order for later whole-schema round trips.
text = project.read_text()
test_anchor = '''    #[test]
    fn truncated_view_configuration_is_refused() {
'''
observer_test = '''    #[test]
    fn provider_option_order_is_preserved_for_schema_round_trip() {
        let value = json!({
            "__typename": "ProjectV2SingleSelectField",
            "id": "PVTSSF_status",
            "databaseId": 101,
            "name": "Status",
            "dataType": "SINGLE_SELECT",
            "updatedAt": "2026-09-17T20:00:00Z",
            "options": [
                {"id": "z-option", "name": "Last alphabetically", "description": "foreign-z", "color": "PINK"},
                {"id": "a-option", "name": "First alphabetically", "description": "foreign-a", "color": "BLUE"}
            ]
        });
        let field = parse_field(&value).unwrap();
        assert_eq!(
            field.options.iter().map(|option| option.id.as_str()).collect::<Vec<_>>(),
            vec!["z-option", "a-option"]
        );
    }

'''
if test_anchor not in text:
    raise SystemExit("observer option-order test anchor not found")
text = text.replace(test_anchor, observer_test + test_anchor, 1)
project.write_text(text)

# Planner: managed option additions/drift become one schema mutation; existing targets and missing identities stay review-only.
core = Path("crates/allodium-core/src/github_project.rs")
text = core.read_text()
old = '''        if field.record.kind == "single_select" {
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
'''
new = '''        if field.record.kind == "single_select" {
            let Some(provider_field) = provider_state
                .fields
                .iter()
                .find(|candidate| candidate.node_id == field_mapping.node_id)
            else {
                operations.push(project_operation(
                    &board.record.id,
                    "observe_project",
                    Some(mapping.number),
                    vec![format!("field-provider-state:{}", field.record.id)],
                    "mapped ProjectV2 single-select field is absent from the persisted provider snapshot; refresh observation before schema mutation",
                ));
                return Ok(());
            };
            let mut update_options = false;
            for option in &field.record.options {
                let marker = canonical_option_marker(&field.record.id, &option.id);
                match field_mapping.options.get(&option.id) {
                    Some(provider_id) => {
                        let Some(provider_option) = provider_field
                            .options
                            .iter()
                            .find(|candidate| candidate.id == *provider_id)
                        else {
                            operations.push(runtime_requirement(
                                &board.record.id,
                                Some(mapping.number),
                                vec![format!(
                                    "field-option-identity-review:{}:{}",
                                    field.record.id, option.id
                                )],
                                "mapped single-select option is absent from the observed provider schema; refusing name/marker-based identity recovery",
                            ));
                            return Ok(());
                        };
                        if provider_option.name != option.name
                            || provider_option.description != marker
                        {
                            if binding.target == "managed" {
                                update_options = true;
                            } else {
                                operations.push(runtime_requirement(
                                    &board.record.id,
                                    Some(mapping.number),
                                    vec![format!(
                                        "field-option-drift-review:{}:{}",
                                        field.record.id, option.id
                                    )],
                                    "existing-target single-select option differs from canonical managed metadata; automatic provider schema mutation is disabled",
                                ));
                                return Ok(());
                            }
                        }
                    }
                    None => {
                        if provider_field
                            .options
                            .iter()
                            .any(|candidate| candidate.description == marker)
                        {
                            operations.push(runtime_requirement(
                                &board.record.id,
                                Some(mapping.number),
                                vec![format!(
                                    "field-option-marker-review:{}:{}",
                                    field.record.id, option.id
                                )],
                                "unmapped provider option already carries the canonical Allodium marker; refusing to adopt provider identity implicitly",
                            ));
                            return Ok(());
                        }
                        if binding.target == "managed" {
                            update_options = true;
                        } else {
                            operations.push(runtime_requirement(
                                &board.record.id,
                                Some(mapping.number),
                                vec![format!("field-option:{}:{}", field.record.id, option.id)],
                                "existing-target canonical single-select option has no explicit stable provider option identity",
                            ));
                            return Ok(());
                        }
                    }
                }
            }
            if update_options {
                operations.push(project_operation(
                    &board.record.id,
                    "update_project_field_options",
                    Some(mapping.number),
                    vec![format!("field:{}", field.record.id)],
                    "managed single-select provider schema differs from canonical options; replace the full option schema while preserving every observed provider option identity",
                ));
                return Ok(());
            }
        }
'''
if old not in text:
    raise SystemExit("single-select planner boundary anchor not found")
text = text.replace(old, new, 1)

helper_anchor = '''fn expected_provider_value(
'''
helper = '''fn canonical_option_marker(field_id: &str, option_id: &str) -> String {
    format!("allodium:{field_id}:{option_id}")
}

'''
if helper_anchor not in text:
    raise SystemExit("core option helper anchor not found")
text = text.replace(helper_anchor, helper + helper_anchor, 1)

# Fix the old matching fixture marker now that option semantics are actually audited.
text = text.replace(
    'description: "allodium:status:active".into(),\n                    color: "BLUE".into(),',
    'description: "allodium:status:todo".into(),\n                    color: "BLUE".into(),',
    1,
)

# Planner tests for managed evolution and existing-target refusal.
test_anchor = '''    #[test]
    fn managed_board_view_creation_uses_stable_field_bridge_and_ignores_same_name_foreign_view() {
'''
tests = '''    #[test]
    fn managed_single_select_addition_plans_preservation_aware_schema_update() {
        let root = ready_root("option-add", "managed", None);
        write_project_mapping(&root, true);
        write_observed_project_fixture(&root);
        write_provider_state_fixture(&root, true);
        write_issue_mapping(&root);
        write_content_identity(&root);
        let path = root.join(".project/boards/board-0001/fields/status.toml");
        let mut field = fs::read_to_string(&path).unwrap();
        field.push_str("\\n[[options]]\\nid = \\\"doing\\\"\\nname = \\\"Doing\\\"\\n");
        fs::write(path, field).unwrap();
        let plan = plan_boards(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "update_project_field_options");
        assert_eq!(plan.operations[0].fields, vec!["field:status"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_target_option_addition_stays_review_only() {
        let root = ready_root("option-existing", "existing", Some(7));
        write_project_mapping(&root, true);
        write_observed_project_fixture(&root);
        write_provider_state_fixture(&root, true);
        write_issue_mapping(&root);
        write_content_identity(&root);
        let path = root.join(".project/boards/board-0001/fields/status.toml");
        let mut field = fs::read_to_string(&path).unwrap();
        field.push_str("\\n[[options]]\\nid = \\\"doing\\\"\\nname = \\\"Doing\\\"\\n");
        fs::write(path, field).unwrap();
        let plan = plan_boards(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "projects_runtime_required");
        assert!(plan.operations[0].fields.contains(&"field-option:status:doing".into()));
        fs::remove_dir_all(root).unwrap();
    }

'''
if test_anchor not in text:
    raise SystemExit("core planner test anchor not found")
text = text.replace(test_anchor, tests + test_anchor, 1)
core.write_text(text)

# Runtime mutation + mapping refresh.
runtime = Path("crates/allodium-github/src/project_runtime.rs")
text = runtime.read_text()
text = text.replace(
    'use allodium_core::github_project_observation::load_observed_project_provider_state;',
    'use allodium_core::github_project_observation::{\n    ObservedProviderField, load_observed_project_provider_state,\n};',
    1,
)
text = text.replace(
    '    FieldCreated,\n',
    '    FieldCreated,\n    FieldOptionsUpdated,\n',
    1,
)
text = text.replace(
    '        "create_project_field" => create_field(adapter, root, remote_name, operation),\n',
    '        "create_project_field" => create_field(adapter, root, remote_name, operation),\n        "update_project_field_options" => {\n            update_field_options(adapter, root, remote_name, operation)\n        }\n',
    1,
)
insert_anchor = '''fn add_item(
'''
mutation = r'''fn update_field_options(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    operation: &GitHubOperation,
) -> Result<ProjectsApplyOutcome, String> {
    let (config, token, _owner_node_id) = authorized_context(adapter, root, remote_name)?;
    let board = board(root, &operation.canonical_id)?;
    if binding(&config, &board.record.id)?.target != "managed" {
        return Err("automatic ProjectV2 option-schema mutation is restricted to managed targets".into());
    }
    let field_id = tagged(operation, "field:")?;
    let field = board_field(&board, field_id)?;
    if field.record.kind != "single_select" {
        return Err("update_project_field_options requires a canonical single_select field".into());
    }
    let project_mapping =
        require_fresh_project(adapter, root, remote_name, &token, &board.record.id)?;
    let field_mapping = project_mapping
        .fields
        .get(field_id)
        .cloned()
        .ok_or_else(|| "ProjectV2 field mapping disappeared after freshness check".to_string())?;
    let observed = load_observed_project_provider_state(root, remote_name, &board.record.id)?
        .ok_or_else(|| "ProjectV2 provider-state observation disappeared after freshness check".to_string())?;
    let provider_field = observed
        .fields
        .iter()
        .find(|candidate| candidate.node_id == field_mapping.node_id)
        .cloned()
        .ok_or_else(|| "mapped ProjectV2 field is absent from provider-state observation".to_string())?;

    let options = merged_single_select_options(field, &field_mapping, &provider_field)?;
    let query = r#"
        mutation($field: ID!, $options: [ProjectV2SingleSelectFieldOptionInput!]!) {
          updateProjectV2Field(input: {fieldId: $field, singleSelectOptions: $options}) {
            projectV2Field {
              ... on ProjectV2SingleSelectField {
                id name dataType options { id name description color }
              }
            }
          }
        }
    "#;
    let payload = project::graphql(
        adapter,
        &token,
        query,
        json!({"field": field_mapping.node_id, "options": options}),
    )?;
    project::graphql_errors(&payload)?;
    let updated = payload
        .pointer("/data/updateProjectV2Field/projectV2Field")
        .ok_or_else(|| "GitHub updateProjectV2Field returned no single-select field".to_string())?;
    if required_str(updated, "id")? != field_mapping.node_id {
        return Err("updated ProjectV2 field identity changed unexpectedly".into());
    }
    let returned = updated
        .get("options")
        .and_then(Value::as_array)
        .ok_or_else(|| "updated ProjectV2 field returned no options".to_string())?;

    for previous in &provider_field.options {
        if !returned
            .iter()
            .any(|candidate| candidate.get("id").and_then(Value::as_str) == Some(previous.id.as_str()))
        {
            return Err(format!(
                "GitHub option-schema update did not preserve pre-existing provider option identity {:?}",
                previous.id
            ));
        }
    }

    let mut mappings = load_project_mappings(root, remote_name)?;
    let board_mapping = mappings
        .boards
        .get_mut(&board.record.id)
        .ok_or_else(|| "ProjectV2 mapping disappeared after option-schema mutation".to_string())?;
    let mapped_field = board_mapping
        .fields
        .get_mut(field_id)
        .ok_or_else(|| "ProjectV2 field mapping disappeared after option-schema mutation".to_string())?;
    for canonical in &field.record.options {
        let marker = option_marker(&field.record.id, &canonical.id);
        if let Some(existing_id) = field_mapping.options.get(&canonical.id) {
            let provider = returned
                .iter()
                .find(|candidate| candidate.get("id").and_then(Value::as_str) == Some(existing_id.as_str()))
                .ok_or_else(|| format!("updated mapped option {:?} disappeared", canonical.id))?;
            if provider.get("name").and_then(Value::as_str) != Some(canonical.name.as_str())
                || provider.get("description").and_then(Value::as_str) != Some(marker.as_str())
            {
                return Err(format!(
                    "updated mapped option {:?} returned unexpected managed metadata",
                    canonical.id
                ));
            }
        } else {
            let matches = returned
                .iter()
                .filter(|candidate| {
                    candidate.get("description").and_then(Value::as_str) == Some(marker.as_str())
                })
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err(format!(
                    "new canonical option {:?} could not be identified uniquely by its Allodium marker",
                    canonical.id
                ));
            }
            mapped_field
                .options
                .insert(canonical.id.clone(), required_str(matches[0], "id")?);
        }
    }
    save_project_mappings(root, remote_name, &mappings)?;
    Ok(ProjectsApplyOutcome::FieldOptionsUpdated)
}

fn merged_single_select_options(
    field: &CanonicalBoardField,
    mapping: &ProjectFieldMapping,
    provider_field: &ObservedProviderField,
) -> Result<Vec<Value>, String> {
    let canonical_by_provider = mapping
        .options
        .iter()
        .map(|(canonical, provider)| (provider.as_str(), canonical.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut merged = Vec::new();
    for provider in &provider_field.options {
        if let Some(canonical_id) = canonical_by_provider.get(provider.id.as_str()) {
            if let Some(canonical) = field
                .record
                .options
                .iter()
                .find(|option| option.id == *canonical_id)
            {
                merged.push(json!({
                    "id": provider.id,
                    "name": canonical.name,
                    "description": option_marker(&field.record.id, &canonical.id),
                    "color": provider.color,
                }));
                continue;
            }
        }
        merged.push(json!({
            "id": provider.id,
            "name": provider.name,
            "description": provider.description,
            "color": provider.color,
        }));
    }
    for (index, canonical) in field.record.options.iter().enumerate() {
        if mapping.options.contains_key(&canonical.id) {
            continue;
        }
        let marker = option_marker(&field.record.id, &canonical.id);
        if provider_field
            .options
            .iter()
            .any(|provider| provider.description == marker)
        {
            return Err(format!(
                "unmapped provider option already carries Allodium marker {marker:?}; refusing implicit identity adoption"
            ));
        }
        merged.push(json!({
            "name": canonical.name,
            "description": marker,
            "color": option_color(index),
        }));
    }
    Ok(merged)
}

'''
if insert_anchor not in text:
    raise SystemExit("runtime add_item anchor not found")
text = text.replace(insert_anchor, mutation + insert_anchor, 1)

# Tests: pure preservation/order merge and provider mutation/mapping refresh.
test_anchor = '''    #[test]
    fn stale_project_state_refuses_update_before_mutation() {
'''
tests = r'''    #[test]
    fn option_merge_preserves_provider_order_foreign_and_removed_options() {
        use allodium_core::github_project_observation::ObservedProviderOption;

        let root = test_root("option-merge");
        write_ready_root(&root, "ALLODIUM_RUNTIME_OPTION_MERGE");
        fs::write(
            root.join(".project/boards/board-0001/fields/status.toml"),
            "schema = \"allodium.board-field/v0\"\nid = \"status\"\nname = \"Status\"\nkind = \"single_select\"\n\n[[options]]\nid = \"todo\"\nname = \"To do\"\n\n[[options]]\nid = \"doing\"\nname = \"Doing\"\n",
        )
        .unwrap();
        let board = board(&root, "board-0001").unwrap();
        let field = board_field(&board, "status").unwrap();
        let mapping = ProjectFieldMapping {
            node_id: "PVTSSF_status".into(),
            data_type: "SINGLE_SELECT".into(),
            options: BTreeMap::from([
                ("todo".into(), "provider-todo".into()),
                ("retired".into(), "provider-retired".into()),
            ]),
        };
        let provider = ObservedProviderField {
            node_id: "PVTSSF_status".into(),
            provider_type: "ProjectV2SingleSelectField".into(),
            name: "Status".into(),
            data_type: "SINGLE_SELECT".into(),
            provider_database_id: Some(101),
            remote_updated_at: "2026-09-17T20:00:00Z".into(),
            options: vec![
                ObservedProviderOption { id: "foreign".into(), name: "Foreign".into(), description: "provider".into(), color: "PINK".into() },
                ObservedProviderOption { id: "provider-todo".into(), name: "Todo".into(), description: "allodium:status:todo".into(), color: "BLUE".into() },
                ObservedProviderOption { id: "provider-retired".into(), name: "Retired".into(), description: "allodium:status:retired".into(), color: "GRAY".into() },
            ],
            iterations: Vec::new(),
        };
        let merged = merged_single_select_options(field, &mapping, &provider).unwrap();
        assert_eq!(merged.len(), 4);
        assert_eq!(merged[0]["id"], "foreign");
        assert_eq!(merged[0]["name"], "Foreign");
        assert_eq!(merged[1]["id"], "provider-todo");
        assert_eq!(merged[1]["name"], "To do");
        assert_eq!(merged[1]["color"], "BLUE");
        assert_eq!(merged[2]["id"], "provider-retired");
        assert_eq!(merged[2]["name"], "Retired");
        assert!(merged[3].get("id").is_none());
        assert_eq!(merged[3]["name"], "Doing");
        assert_eq!(merged[3]["description"], "allodium:status:doing");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn managed_option_update_preserves_existing_ids_and_maps_new_marker() {
        use allodium_core::github_project_observation::ObservedProviderOption;

        let root = test_root("option-update");
        write_ready_root(&root, "ALLODIUM_RUNTIME_OPTION_UPDATE");
        fs::write(
            root.join(".project/boards/board-0001/fields/status.toml"),
            "schema = \"allodium.board-field/v0\"\nid = \"status\"\nname = \"Status\"\nkind = \"single_select\"\n\n[[options]]\nid = \"todo\"\nname = \"To do\"\n\n[[options]]\nid = \"doing\"\nname = \"Doing\"\n",
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
                        options: BTreeMap::from([("todo".into(), "provider-todo".into())]),
                    },
                )]),
                views: BTreeMap::new(),
            },
        );
        save_project_mappings(
            &root,
            "github",
            &ProjectMappings { schema: PROJECT_MAPPINGS_SCHEMA_V0.into(), boards },
        )
        .unwrap();
        let provider_options = vec![
            ObservedProviderOption { id: "foreign".into(), name: "Foreign".into(), description: "provider".into(), color: "PINK".into() },
            ObservedProviderOption { id: "provider-todo".into(), name: "Todo".into(), description: "allodium:status:todo".into(), color: "BLUE".into() },
        ];
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
                    options: provider_options,
                    iterations: Vec::new(),
                }],
                items: Vec::new(),
                views: Vec::new(),
            },
        )
        .unwrap();
        unsafe { env::set_var("ALLODIUM_RUNTIME_OPTION_UPDATE", "projects-token") };
        let live = r#"{"data":{"node":{"id":"PVT_project","number":7,"url":"https://github.com/users/sguzman/projects/7","title":"Board","shortDescription":"","closed":false,"updatedAt":"2026-09-17T20:00:00Z","owner":{"id":"U_owner"},"fields":{"nodes":[{"__typename":"ProjectV2SingleSelectField","id":"PVTSSF_status","databaseId":101,"name":"Status","dataType":"SINGLE_SELECT","updatedAt":"2026-09-17T20:00:00Z","options":[{"id":"foreign","name":"Foreign","description":"provider","color":"PINK"},{"id":"provider-todo","name":"Todo","description":"allodium:status:todo","color":"BLUE"}]}],"pageInfo":{"hasNextPage":false}},"views":{"nodes":[],"pageInfo":{"hasNextPage":false}},"items":{"nodes":[],"pageInfo":{"hasNextPage":false}}}}}"#;
        let updated = r#"{"data":{"updateProjectV2Field":{"projectV2Field":{"id":"PVTSSF_status","name":"Status","dataType":"SINGLE_SELECT","options":[{"id":"foreign","name":"Foreign","description":"provider","color":"PINK"},{"id":"provider-todo","name":"To do","description":"allodium:status:todo","color":"BLUE"},{"id":"provider-doing","name":"Doing","description":"allodium:status:doing","color":"BLUE"}]}}}}"#;
        let responses = vec![
            json_response(200, r#"{"data":{"user":{"id":"U_owner","projectsV2":{"totalCount":1}}}}"#),
            json_response(200, live),
            json_response(200, updated),
        ];
        let (base, requests, handle) = response_server(responses);
        let operation = GitHubOperation {
            canonical_id: "board-0001".into(),
            action: "update_project_field_options".into(),
            number: Some(7),
            fields: vec!["field:status".into()],
            reason: "test".into(),
        };
        assert_eq!(
            apply_operation(&test_adapter(base), &root, "github", &operation).unwrap(),
            ProjectsApplyOutcome::FieldOptionsUpdated
        );
        handle.join().unwrap();
        unsafe { env::remove_var("ALLODIUM_RUNTIME_OPTION_UPDATE") };
        let mappings = load_project_mappings(&root, "github").unwrap();
        assert_eq!(mappings.boards["board-0001"].fields["status"].options["todo"], "provider-todo");
        assert_eq!(mappings.boards["board-0001"].fields["status"].options["doing"], "provider-doing");
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(requests[2].contains("authorization: Bearer projects-token"));
        assert!(requests[2].contains("provider-todo"));
        assert!(requests[2].contains("foreign"));
        assert!(requests[2].contains("allodium:status:doing"));
        assert!(!requests[2].contains("repo-token"));
        fs::remove_dir_all(root).unwrap();
    }

'''
if test_anchor not in text:
    raise SystemExit("runtime test anchor not found")
text = text.replace(test_anchor, tests + test_anchor, 1)
runtime.write_text(text)

# Apply report and CLI visibility.
lib = Path("crates/allodium-github/src/lib.rs")
text = lib.read_text()
text = text.replace(
    '    pub project_fields_created: usize,\n',
    '    pub project_fields_created: usize,\n    pub project_field_option_schemas_updated: usize,\n',
    1,
)
text = text.replace(
    '                | "create_project_field"\n',
    '                | "create_project_field"\n                | "update_project_field_options"\n',
    1,
)
text = text.replace(
    '''                        project_runtime::ProjectsApplyOutcome::FieldCreated => {
                            report.project_fields_created += 1
                        }
''',
    '''                        project_runtime::ProjectsApplyOutcome::FieldCreated => {
                            report.project_fields_created += 1
                        }
                        project_runtime::ProjectsApplyOutcome::FieldOptionsUpdated => {
                            report.project_field_option_schemas_updated += 1
                        }
''',
    1,
)
lib.write_text(text)

cli = Path("crates/allodium-cli/src/main.rs")
text = cli.read_text()
anchor = '''            println!(
                "created {} GitHub ProjectV2 field(s)",
                report.project_fields_created
            );
'''
addition = anchor + '''            println!(
                "updated {} GitHub ProjectV2 single-select option schema(s)",
                report.project_field_option_schemas_updated
            );
'''
if anchor not in text:
    raise SystemExit("CLI Project field report anchor not found")
text = text.replace(anchor, addition, 1)
cli.write_text(text)

# Provider documentation for the overwrite-preserving merge contract.
docs = Path("docs/github-projects-provider-v0.md")
text = docs.read_text()
section = '''

## Managed single-select option evolution

GitHub's `updateProjectV2Field` treats a supplied single-select option list as replacement-shaped provider schema. GitHub's option input accepts an existing option `id` specifically to preserve option identity during updates and avoid clearing item values. Allodium therefore never sends a canonical-only option list.

For an explicitly `managed` single-select field, Allodium starts from the complete observed provider option sequence. Every observed option is emitted in the same order and with its existing provider ID. A provider option mapped to a still-present canonical option receives the canonical display name and deterministic `allodium:<field-id>:<option-id>` marker while preserving its existing provider color and provider option ID. Foreign provider options and options whose former canonical counterpart has been removed are emitted unchanged. Canonical absence therefore does not mean provider option deletion.

A newly added canonical option is appended without a provider ID, with its Allodium marker and a deterministic creation color. After the mutation, the returned schema must still contain every pre-existing provider option ID. Existing canonical mappings are verified by their provider IDs, while each new canonical option is mapped only from exactly one returned Allodium marker. A missing mapped option, marker collision, identity disappearance, or ambiguous new marker is a refusal/review boundary rather than a name-based recovery rule.

Automatic option-schema mutation is restricted to managed Project targets. Existing-target fields remain review-only because Allodium has not granted itself authority to rewrite externally owned provider taxonomy. Every option-schema write also passes the normal full-Project optimistic freshness check before mutation and is followed by re-observation in the serialized sync loop.

Provider option order is now preserved by observation rather than sorted by ID. Once the adapter can round-trip whole option schemas, discarding provider order would itself be an unintended schema mutation.
'''
if "## Managed single-select option evolution" not in text:
    text += section
docs.write_text(text)
