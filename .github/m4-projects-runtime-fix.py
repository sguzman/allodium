from pathlib import Path

core = Path("crates/allodium-core/src/github_project.rs")
text = core.read_text()

# Content identity is a read-only prerequisite and should be diagnosed before
# provider field schema work. Membership itself still waits until fields exist.
anchor = "    // Establish provider field identity before item values. Managed targets may\n"
preflight = r'''    // Content node identity can be established independently of Project field
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

'''
if preflight.strip() not in text:
    if anchor not in text:
        raise SystemExit("planner preflight anchor missing")
    text = text.replace(anchor, preflight + anchor, 1)

# Old fixtures represented only the lightweight ObservedProject record. The
# mutation planner now intentionally requires the full normalized provider
# snapshot as its optimistic-write basis.
text = text.replace(
    '''        write_observed_project_fixture(&root);\n        write_issue_mapping(&root);\n        let plan = plan_boards(&root, "github").unwrap();''',
    '''        write_observed_project_fixture(&root);\n        write_provider_state_fixture(&root, false);\n        write_issue_mapping(&root);\n        let plan = plan_boards(&root, "github").unwrap();''',
    1,
)
text = text.replace(
    '''    fn fully_known_identity_still_stops_before_project_mutation() {\n        let root = ready_root("known", "managed", None);\n        write_project_mapping(&root, true);\n        write_observed_project_fixture(&root);\n        write_issue_mapping(&root);\n        write_content_identity(&root);\n        let plan = plan_boards(&root, "github").unwrap();\n        assert_eq!(plan.operations.len(), 1);\n        assert_eq!(plan.operations[0].action, "projects_runtime_required");\n        assert!(plan.operations[0].fields.contains(&"field-values".into()));\n        assert!(!plan.operations[0].action.contains("delete"));\n        fs::remove_dir_all(root).unwrap();\n    }''',
    '''    fn fully_known_and_matching_project_state_is_idempotent() {\n        let root = ready_root("known", "managed", None);\n        write_project_mapping(&root, true);\n        write_observed_project_fixture(&root);\n        write_provider_state_fixture(&root, true);\n        write_issue_mapping(&root);\n        write_content_identity(&root);\n        let plan = plan_boards(&root, "github").unwrap();\n        assert!(plan.operations.is_empty());\n        fs::remove_dir_all(root).unwrap();\n    }''',
    1,
)

helper_anchor = "    fn write_issue_mapping(root: &Path) {\n"
helper = r'''    fn write_provider_state_fixture(root: &Path, complete: bool) {
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
                remote_updated_at: "2026-09-17T00:00:00Z".into(),
                options: vec![ObservedProviderOption {
                    id: "provider-option".into(),
                    name: "Todo".into(),
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
                name: "Development".into(),
                layout: "BOARD_LAYOUT".into(),
                filter: None,
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

'''
if "fn write_provider_state_fixture" not in text:
    if helper_anchor not in text:
        raise SystemExit("provider-state fixture anchor missing")
    text = text.replace(helper_anchor, helper + helper_anchor, 1)
core.write_text(text)

runtime = Path("crates/allodium-github/src/project_runtime.rs")
text = runtime.read_text()
text = text.replace(
    "    ObservedProjectProviderState, load_observed_project_provider_state,\n",
    "    load_observed_project_provider_state,\n",
    1,
)
runtime.write_text(text)
