from pathlib import Path

project = Path("crates/allodium-github/src/project.rs")
text = project.read_text()
text = text.replace(
    "use allodium_core::board::{CanonicalBoard, load_boards};",
    "use allodium_core::board::load_boards;",
    1,
)
text = text.replace(
    "    ObservedProjectsCapabilities, ProjectItemMapping, ProjectMapping, ProjectMappings,\n",
    "    ObservedProjectsCapabilities, ProjectItemMapping, ProjectMapping,\n",
    1,
)
marker = "pub(super) fn observe_projects(\n"
compat = r'''pub(super) fn observe_projects_capabilities(
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
    if !boards.iter().any(|board| {
        config
            .boards
            .get(&board.record.id)
            .is_some_and(|binding| binding.enabled)
    }) {
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
    write_observed_projects_capabilities(
        root,
        remote_name,
        &ObservedProjectsCapabilities {
            schema: OBSERVED_PROJECTS_CAPABILITIES_SCHEMA_V0.into(),
            credential_available: token.is_some(),
            authenticated,
            owner_kind: config.owner_kind,
            owner: config.owner,
            owner_node_id,
            observed_at: now(),
        },
    )?;
    Ok(ProjectsObserveReport {
        capabilities_observed: 1,
        ..ProjectsObserveReport::default()
    })
}

'''
if "pub(super) fn observe_projects_capabilities(" not in text:
    if marker not in text:
        raise SystemExit("observe_projects insertion point not found")
    text = text.replace(marker, compat + marker, 1)

old_second = '''        let second = provider_project_json("Doing", "Draft edited", "2026-09-17T20:02:00Z");
'''
new_second = '''        let second = provider_project_node_json("Doing", "Draft edited", "2026-09-17T20:02:00Z");
'''
if old_second not in text:
    raise SystemExit("ProjectV2 drift test second-observation fixture not found")
text = text.replace(old_second, new_second, 1)

function_marker = '''    fn test_root(name: &str) -> PathBuf {
'''
node_fixture = r'''    fn provider_project_node_json(
        option_name: &str,
        draft_title: &str,
        updated_at: &str,
    ) -> String {
        let document: serde_json::Value =
            serde_json::from_str(&provider_project_json(option_name, draft_title, updated_at))
                .unwrap();
        let project = document
            .pointer("/data/user/projectV2")
            .expect("provider project fixture has user.projectV2")
            .clone();
        json!({ "data": { "node": project } }).to_string()
    }

'''
if "fn provider_project_node_json(" not in text:
    if function_marker not in text:
        raise SystemExit("ProjectV2 test helper insertion point not found")
    text = text.replace(function_marker, node_fixture + function_marker, 1)
project.write_text(text)

adapter = Path("crates/allodium-github/src/lib.rs")
text = adapter.read_text()
replacements = [
    (
        "        let issue = ApiIssue {\n            number: 7,\n",
        "        let issue = ApiIssue {\n            number: 7,\n            node_id: \"I_test_observe\".into(),\n",
    ),
    (
        "        let old = ApiIssue {\n            number: 7,\n",
        "        let old = ApiIssue {\n            number: 7,\n            node_id: \"I_test_drift\".into(),\n",
    ),
    (
        "        let review = ApiPullRequest {\n            number: 23,\n",
        "        let review = ApiPullRequest {\n            number: 23,\n            node_id: \"PR_test_observe\".into(),\n",
    ),
]
for old, new in replacements:
    if old not in text:
        raise SystemExit(f"expected struct fixture not found: {old!r}")
    text = text.replace(old, new, 1)
adapter.write_text(text)
