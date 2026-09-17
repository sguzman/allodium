from pathlib import Path

# Enrich read-only ProjectV2 provider snapshots so later schema mutations can
# round-trip foreign options/views without guessing or discarding provider data.
core = Path("crates/allodium-core/src/github_project_observation.rs")
text = core.read_text()

text = text.replace(
'''pub struct ObservedProviderOption {
    pub id: String,
    pub name: String,
}
''',
'''pub struct ObservedProviderOption {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub color: String,
}
''',
1,
)
text = text.replace(
'''pub struct ObservedProviderField {
    pub node_id: String,
    pub provider_type: String,
    pub name: String,
    pub data_type: String,
    pub remote_updated_at: String,
''',
'''pub struct ObservedProviderField {
    pub node_id: String,
    pub provider_type: String,
    pub name: String,
    pub data_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_database_id: Option<i64>,
    pub remote_updated_at: String,
''',
1,
)
text = text.replace(
'''#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedProviderView {
    pub node_id: String,
    pub number: u64,
    pub name: String,
    pub layout: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}
''',
'''#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedProviderViewSort {
    pub field_node_id: String,
    pub direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedProviderView {
    pub node_id: String,
    pub number: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_database_id: Option<String>,
    pub name: String,
    pub layout: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(default)]
    pub remote_updated_at: String,
    #[serde(default)]
    pub visible_field_node_ids: Vec<String>,
    #[serde(default)]
    pub group_by_field_node_ids: Vec<String>,
    #[serde(default)]
    pub vertical_group_by_field_node_ids: Vec<String>,
    #[serde(default)]
    pub sort_by: Vec<ObservedProviderViewSort>,
}
''',
1,
)
# Update core test literals while retaining backwards-compatible serde defaults.
text = text.replace(
'''                data_type: "SINGLE_SELECT".into(),
                remote_updated_at: "2026-09-17T19:59:00Z".into(),
                options: vec![ObservedProviderOption {
                    id: "option-1".into(),
                    name: option_name.into(),
                }],
''',
'''                data_type: "SINGLE_SELECT".into(),
                provider_database_id: Some(101),
                remote_updated_at: "2026-09-17T19:59:00Z".into(),
                options: vec![ObservedProviderOption {
                    id: "option-1".into(),
                    name: option_name.into(),
                    description: "provider-owned description".into(),
                    color: "BLUE".into(),
                }],
''',
1,
)
core.write_text(text)

project = Path("crates/allodium-github/src/project.rs")
text = project.read_text()
text = text.replace(
'''            ... on ProjectV2Field { id name dataType updatedAt }
            ... on ProjectV2SingleSelectField { id name dataType updatedAt options { id name } }
            ... on ProjectV2IterationField {
              id name dataType updatedAt
''',
'''            ... on ProjectV2Field { id databaseId name dataType updatedAt }
            ... on ProjectV2SingleSelectField {
              id databaseId name dataType updatedAt
              options { id name description color }
            }
            ... on ProjectV2IterationField {
              id databaseId name dataType updatedAt
''',
1,
)
text = text.replace(
'''        views(first: 100) {
          nodes { id number name layout filter }
          pageInfo { hasNextPage }
        }
''',
'''        views(first: 100) {
          nodes {
            id fullDatabaseId number name layout filter updatedAt
            fields(first: 100) {
              nodes { ... on ProjectV2FieldCommon { id } }
              pageInfo { hasNextPage }
            }
            groupByFields(first: 100) {
              nodes { ... on ProjectV2FieldCommon { id } }
              pageInfo { hasNextPage }
            }
            verticalGroupByFields(first: 100) {
              nodes { ... on ProjectV2FieldCommon { id } }
              pageInfo { hasNextPage }
            }
            sortByFields(first: 100) {
              nodes {
                direction
                field { ... on ProjectV2FieldCommon { id } }
              }
              pageInfo { hasNextPage }
            }
          }
          pageInfo { hasNextPage }
        }
''',
1,
)
text = text.replace(
'''            Ok(ObservedProviderOption {
                id: required_string(option, "id")?,
                name: required_string(option, "name")?,
            })
''',
'''            Ok(ObservedProviderOption {
                id: required_string(option, "id")?,
                name: required_string(option, "name")?,
                description: required_string(option, "description")?,
                color: required_string(option, "color")?,
            })
''',
1,
)
text = text.replace(
'''        data_type: required_string(value, "dataType")?,
        remote_updated_at: required_string(value, "updatedAt")?,
''',
'''        data_type: required_string(value, "dataType")?,
        provider_database_id: value.get("databaseId").and_then(Value::as_i64),
        remote_updated_at: required_string(value, "updatedAt")?,
''',
1,
)
old_view = '''fn parse_view(value: &Value) -> Result<ObservedProviderView, String> {
    Ok(ObservedProviderView {
        node_id: required_string(value, "id")?,
        number: required_u64(value, "number")?,
        name: required_string(value, "name")?,
        layout: required_string(value, "layout")?,
        filter: value
            .get("filter")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}
'''
new_view = '''fn parse_view(value: &Value) -> Result<ObservedProviderView, String> {
    let visible_field_node_ids = parse_view_field_connection(value.get("fields"), "view fields")?;
    let group_by_field_node_ids =
        parse_view_field_connection(value.get("groupByFields"), "view groupByFields")?;
    let vertical_group_by_field_node_ids = parse_view_field_connection(
        value.get("verticalGroupByFields"),
        "view verticalGroupByFields",
    )?;
    reject_truncated_connection(value.get("sortByFields"), "view sortByFields")?;
    let sort_by = value
        .pointer("/sortByFields/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| !entry.is_null())
        .map(|entry| {
            Ok(ObservedProviderViewSort {
                field_node_id: entry
                    .pointer("/field/id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "ProjectV2 view sort entry is missing field.id".to_string())?
                    .into(),
                direction: required_string(entry, "direction")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ObservedProviderView {
        node_id: required_string(value, "id")?,
        number: required_u64(value, "number")?,
        full_database_id: optional_bigint_string(value.get("fullDatabaseId")),
        name: required_string(value, "name")?,
        layout: required_string(value, "layout")?,
        filter: value
            .get("filter")
            .and_then(Value::as_str)
            .map(str::to_owned),
        remote_updated_at: required_string(value, "updatedAt")?,
        visible_field_node_ids,
        group_by_field_node_ids,
        vertical_group_by_field_node_ids,
        sort_by,
    })
}

fn parse_view_field_connection(value: Option<&Value>, name: &str) -> Result<Vec<String>, String> {
    reject_truncated_connection(value, name)?;
    value
        .and_then(|connection| connection.get("nodes"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|node| !node.is_null())
        .map(|node| {
            node.get("id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| format!("GitHub ProjectV2 {name} entry is missing id"))
        })
        .collect()
}

fn optional_bigint_string(value: Option<&Value>) -> Option<String> {
    value.and_then(|value| {
        value
            .as_str()
            .map(str::to_owned)
            .or_else(|| value.as_i64().map(|number| number.to_string()))
            .or_else(|| value.as_u64().map(|number| number.to_string()))
    })
}
'''
if old_view not in text:
    raise SystemExit("parse_view anchor missing")
text = text.replace(old_view, new_view, 1)

# Enrich authorized provider fixture and assert that provider schema data survives.
text = text.replace(
'''                                    "id": "PVTSSF_status",
                                    "name": "Status",
                                    "dataType": "SINGLE_SELECT",
                                    "updatedAt": updated_at,
                                    "options": [{ "id": "option-1", "name": option_name }]
''',
'''                                    "id": "PVTSSF_status",
                                    "databaseId": 101,
                                    "name": "Status",
                                    "dataType": "SINGLE_SELECT",
                                    "updatedAt": updated_at,
                                    "options": [{
                                        "id": "option-1",
                                        "name": option_name,
                                        "description": "provider-owned option",
                                        "color": "BLUE"
                                    }]
''',
1,
)
text = text.replace(
'''                                    "id": "PVTIF_iteration",
                                    "name": "Iteration",
''',
'''                                    "id": "PVTIF_iteration",
                                    "databaseId": 102,
                                    "name": "Iteration",
''',
1,
)
text = text.replace(
'''                        "views": {
                            "nodes": [{ "id": "PVTV_view", "number": 1, "name": "Board", "layout": "BOARD_LAYOUT", "filter": "" }],
                            "pageInfo": { "hasNextPage": false }
                        },
''',
'''                        "views": {
                            "nodes": [{
                                "id": "PVTV_view",
                                "fullDatabaseId": "9007199254740993",
                                "number": 1,
                                "name": "Board",
                                "layout": "BOARD_LAYOUT",
                                "filter": "",
                                "updatedAt": updated_at,
                                "fields": {
                                    "nodes": [{ "id": "PVTSSF_status" }, { "id": "PVTIF_iteration" }],
                                    "pageInfo": { "hasNextPage": false }
                                },
                                "groupByFields": {
                                    "nodes": [],
                                    "pageInfo": { "hasNextPage": false }
                                },
                                "verticalGroupByFields": {
                                    "nodes": [{ "id": "PVTSSF_status" }],
                                    "pageInfo": { "hasNextPage": false }
                                },
                                "sortByFields": {
                                    "nodes": [{ "direction": "ASC", "field": { "id": "PVTIF_iteration" } }],
                                    "pageInfo": { "hasNextPage": false }
                                }
                            }],
                            "pageInfo": { "hasNextPage": false }
                        },
''',
1,
)
assert_anchor = '''        assert!(
            snapshot.fields[0]
                .iterations
                .iter()
                .any(|iteration| iteration.id == "iteration-1")
        );
'''
assert_extra = assert_anchor + '''        let status = snapshot
            .fields
            .iter()
            .find(|field| field.node_id == "PVTSSF_status")
            .unwrap();
        assert_eq!(status.provider_database_id, Some(101));
        assert_eq!(status.options[0].description, "provider-owned option");
        assert_eq!(status.options[0].color, "BLUE");
        let view = &snapshot.views[0];
        assert_eq!(view.full_database_id.as_deref(), Some("9007199254740993"));
        assert_eq!(view.vertical_group_by_field_node_ids, vec!["PVTSSF_status"]);
        assert_eq!(view.visible_field_node_ids, vec!["PVTSSF_status", "PVTIF_iteration"]);
        assert_eq!(view.sort_by[0].field_node_id, "PVTIF_iteration");
        assert_eq!(view.sort_by[0].direction, "ASC");
'''
if assert_anchor not in text:
    raise SystemExit("authorized observation assertion anchor missing")
text = text.replace(assert_anchor, assert_extra, 1)

# Add a parser-level truncation test for nested view configuration.
test_anchor = '''    #[test]
    fn content_identity_is_recorded_only_for_objects_on_enabled_boards() {
'''
new_test = '''    #[test]
    fn truncated_view_configuration_is_refused() {
        let project = provider_project_json("Todo", "Draft", "2026-09-17T20:00:00Z");
        let mut value: Value = serde_json::from_str(&project).unwrap();
        *value
            .pointer_mut("/data/user/projectV2/views/nodes/0/verticalGroupByFields/pageInfo/hasNextPage")
            .unwrap() = Value::Bool(true);
        let error = parse_project(value.pointer("/data/user/projectV2").unwrap()).unwrap_err();
        assert!(error.contains("verticalGroupByFields"));
        assert!(error.contains("silently truncated"));
    }

'''
if new_test.strip() not in text:
    if test_anchor not in text:
        raise SystemExit("test insertion anchor missing")
    text = text.replace(test_anchor, new_test + test_anchor, 1)
project.write_text(text)

# Runtime fixture literals must provide the newly observed provider-only fields.
runtime = Path("crates/allodium-github/src/project_runtime.rs")
text = runtime.read_text()
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
runtime.write_text(text)

# Document why this richer read-only snapshot is a prerequisite for later writes.
docs = Path("docs/github-projects-provider-v0.md")
text = docs.read_text()
section = r'''
## Provider schema-preservation boundary

ProjectV2 schema mutation is intentionally narrower than provider observation. Allodium now preserves enough provider-only schema detail to make a later mutation decision without reconstructing identity from display names or discarding foreign state:

- single-select option ID, name, description, and color;
- custom-field GraphQL node ID plus the provider database ID as non-authoritative bridge evidence;
- view node ID, number, full database ID, update revision, visible field IDs, horizontal group-by field IDs, vertical group-by field IDs, and ordered sort field/direction pairs.

GraphQL node IDs remain the stable provider identities. Numeric/full database IDs are evidence for provider API bridges only and must never replace node IDs as mapping authority.

The distinction matters because GitHub's GraphQL view mutation input can configure visible field IDs but does not expose the richer grouping/sort shape that canonical Allodium views may require. GitHub's REST Project-view surface can express group-by, vertical-group-by, visible fields, and sorting, but it uses database identifiers and has owner/credential-specific support. A canonical board view therefore must not be declared projected merely because a same-named GitHub view exists or because a partial GraphQL view was created. In particular, Allodium's canonical board `group_by` describes kanban columns and must round-trip to the provider's vertical grouping semantics; the canonical roadmap start/end-field semantics must remain deferred until an audited provider mutation can represent them without loss.

Single-select option evolution has a similar whole-schema hazard. GitHub's field-update mutation treats the submitted option list as replacement configuration, and existing provider option IDs must be supplied to preserve option identity and item values. Allodium therefore observes the complete option identity/details before any option-schema update is allowed. Foreign provider options are evidence, not canonical options, and canonical option absence has no deletion meaning in v0.
'''
if "## Provider schema-preservation boundary" not in text:
    text = text.rstrip() + "\n\n" + section.strip() + "\n"
docs.write_text(text)
