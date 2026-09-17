from pathlib import Path

core = Path("crates/allodium-core/src/github_project.rs")
text = core.read_text()
text = text.replace(
'''                data_type: "SINGLE_SELECT".into(),
                remote_updated_at: "2026-09-17T00:00:00Z".into(),
                options: vec![ObservedProviderOption {
                    id: "provider-option".into(),
                    name: "Todo".into(),
                }],
''',
'''                data_type: "SINGLE_SELECT".into(),
                provider_database_id: Some(101),
                remote_updated_at: "2026-09-17T00:00:00Z".into(),
                options: vec![ObservedProviderOption {
                    id: "provider-option".into(),
                    name: "Todo".into(),
                    description: "allodium:status:active".into(),
                    color: "BLUE".into(),
                }],
''',
1,
)
text = text.replace(
'''            vec![ObservedProviderView {
                node_id: "PVTV_view".into(),
                number: 1,
                name: "Development".into(),
                layout: "BOARD_LAYOUT".into(),
                filter: None,
            }]
''',
'''            vec![ObservedProviderView {
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
''',
1,
)
core.write_text(text)

project = Path("crates/allodium-github/src/project.rs")
project_text = project.read_text()
old_import = "    ObservedProviderItem, ObservedProviderIteration, ObservedProviderOption, ObservedProviderView,\n"
new_import = "    ObservedProviderItem, ObservedProviderIteration, ObservedProviderOption, ObservedProviderView,\n    ObservedProviderViewSort,\n"
if old_import not in project_text:
    raise SystemExit("expected ProjectV2 observation import anchor not found")
project.write_text(project_text.replace(old_import, new_import, 1))
