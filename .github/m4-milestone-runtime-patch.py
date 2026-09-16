from pathlib import Path

module = r'''use super::{GitHubAdapter, incoming_directory, now, timestamp_slug, write_toml};
use allodium_core::github_milestone::{
    OBSERVED_MILESTONE_SCHEMA_V0, MilestoneMapping, ObservedMilestone,
    load_milestone_mappings, load_observed_milestone, save_milestone_mappings,
    write_observed_milestone_snapshot,
};
use allodium_core::milestone::CanonicalMilestone;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::Path;

const MILESTONE_CHANGE_SCHEMA_V0: &str =
    "allodium.github.milestone-managed-change-observation/v0";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct MilestoneObserveReport {
    pub observed: usize,
    pub managed_changes_archived: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiMilestone {
    number: u64,
    html_url: String,
    title: String,
    description: Option<String>,
    state: String,
    due_on: Option<String>,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct MilestoneManagedChangeEvent {
    schema: String,
    id: String,
    remote: String,
    kind: String,
    observed_at: String,
    evidence: String,
    canonical_id: String,
    milestone_number: u64,
    url: String,
    fields: Vec<String>,
}

pub(super) fn observe_milestones(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
) -> Result<MilestoneObserveReport, String> {
    let mappings = load_milestone_mappings(root, remote_name)?;
    let mut report = MilestoneObserveReport::default();

    for (canonical_id, mapping) in mappings.milestones {
        let observed_at = now();
        let live = fetch_milestone(adapter, mapping.number)?;
        let (snapshot, description) = snapshot_from_api(&canonical_id, &live, &observed_at);
        report.managed_changes_archived +=
            archive_managed_change_if_needed(root, remote_name, &snapshot, &description)?;
        write_observed_milestone_snapshot(root, remote_name, &snapshot, &description)?;
        report.observed += 1;
    }

    Ok(report)
}

pub(super) fn apply_create_milestone(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    milestone: &CanonicalMilestone,
) -> Result<(), String> {
    adapter.require_write_token()?;
    let mut mappings = load_milestone_mappings(root, remote_name)?;
    if mappings.milestones.contains_key(&milestone.record.id) {
        return Err(format!(
            "refusing create: canonical milestone {:?} already has a GitHub milestone mapping",
            milestone.record.id
        ));
    }

    let created: ApiMilestone = adapter.post(
        &format!("/repos/{}/milestones", adapter.repository),
        &create_payload(milestone),
    )?;
    let observed_at = now();
    let (snapshot, description) = snapshot_from_api(&milestone.record.id, &created, &observed_at);

    mappings.milestones.insert(
        milestone.record.id.clone(),
        MilestoneMapping {
            number: created.number,
            url: created.html_url.clone(),
        },
    );
    save_milestone_mappings(root, remote_name, &mappings)?;
    write_observed_milestone_snapshot(root, remote_name, &snapshot, &description)
}

pub(super) fn apply_observe_milestone(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    number: u64,
) -> Result<(), String> {
    let mappings = load_milestone_mappings(root, remote_name)?;
    let mapping = mappings.milestones.get(canonical_id).ok_or_else(|| {
        format!(
            "cannot observe GitHub milestone {number}: no mapping exists for {canonical_id:?}"
        )
    })?;
    if mapping.number != number {
        return Err(format!(
            "GitHub milestone operation number {number} disagrees with mapping {} for {canonical_id:?}",
            mapping.number
        ));
    }

    let live = fetch_milestone(adapter, number)?;
    let observed_at = now();
    let (snapshot, description) = snapshot_from_api(canonical_id, &live, &observed_at);
    archive_managed_change_if_needed(root, remote_name, &snapshot, &description)?;
    write_observed_milestone_snapshot(root, remote_name, &snapshot, &description)
}

pub(super) fn apply_update_milestone(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    milestone: &CanonicalMilestone,
    number: u64,
    fields: &[String],
) -> Result<(), String> {
    adapter.require_write_token()?;
    let (previous, previous_description) =
        load_observed_milestone(root, remote_name, &milestone.record.id)?.ok_or_else(|| {
            format!(
                "refusing update of {:?}: no observed GitHub milestone snapshot; observe and re-plan before applying",
                milestone.record.id
            )
        })?;
    if previous.number != number {
        return Err(format!(
            "refusing update of {:?}: plan milestone number {number} disagrees with observed number {}",
            milestone.record.id, previous.number
        ));
    }

    let live = fetch_milestone(adapter, number)?;
    let live_observed_at = now();
    let (live_snapshot, live_description) =
        snapshot_from_api(&milestone.record.id, &live, &live_observed_at);
    if !same_remote_state(
        &previous,
        &previous_description,
        &live_snapshot,
        &live_description,
    ) {
        return Err(format!(
            "refusing stale update of {:?}: GitHub milestone changed after the recorded observation; observe and re-plan before applying",
            milestone.record.id
        ));
    }

    let updated: ApiMilestone = adapter.patch(
        &format!("/repos/{}/milestones/{number}", adapter.repository),
        &update_payload(milestone, fields)?,
    )?;
    let observed_at = now();
    let (snapshot, description) = snapshot_from_api(&milestone.record.id, &updated, &observed_at);
    write_observed_milestone_snapshot(root, remote_name, &snapshot, &description)
}

fn fetch_milestone(adapter: &GitHubAdapter, number: u64) -> Result<ApiMilestone, String> {
    adapter.get(&format!(
        "/repos/{}/milestones/{number}",
        adapter.repository
    ))
}

fn create_payload(milestone: &CanonicalMilestone) -> serde_json::Value {
    json!({
        "title": milestone.record.title,
        "state": milestone.record.state,
        "description": milestone.description,
        "due_on": canonical_due_on(milestone.record.due.as_deref()),
    })
}

fn update_payload(
    milestone: &CanonicalMilestone,
    fields: &[String],
) -> Result<serde_json::Value, String> {
    let mut object = serde_json::Map::new();
    for field in fields {
        match field.as_str() {
            "title" => {
                object.insert("title".into(), json!(milestone.record.title));
            }
            "description" => {
                object.insert("description".into(), json!(milestone.description));
            }
            "state" => {
                object.insert("state".into(), json!(milestone.record.state));
            }
            "due" => {
                object.insert(
                    "due_on".into(),
                    json!(canonical_due_on(milestone.record.due.as_deref())),
                );
            }
            other => {
                return Err(format!(
                    "GitHub milestone projection v0 cannot update field {other:?}"
                ));
            }
        }
    }
    Ok(serde_json::Value::Object(object))
}

fn canonical_due_on(due: Option<&str>) -> Option<String> {
    due.map(|date| format!("{date}T23:59:59Z"))
}

fn snapshot_from_api(
    canonical_id: &str,
    milestone: &ApiMilestone,
    observed_at: &str,
) -> (ObservedMilestone, String) {
    (
        ObservedMilestone {
            schema: OBSERVED_MILESTONE_SCHEMA_V0.into(),
            canonical_id: canonical_id.into(),
            number: milestone.number,
            url: milestone.html_url.clone(),
            title: milestone.title.clone(),
            state: milestone.state.clone(),
            due_on: milestone.due_on.clone(),
            remote_updated_at: milestone.updated_at.clone(),
            observed_at: observed_at.into(),
        },
        milestone.description.clone().unwrap_or_default(),
    )
}

fn archive_managed_change_if_needed(
    root: &Path,
    remote_name: &str,
    current: &ObservedMilestone,
    current_description: &str,
) -> Result<usize, String> {
    let Some((previous, previous_description)) =
        load_observed_milestone(root, remote_name, &current.canonical_id)?
    else {
        return Ok(0);
    };
    let fields = changed_fields(
        &previous,
        &previous_description,
        current,
        current_description,
    );
    if fields.is_empty() {
        return Ok(0);
    }

    let event_id = format!(
        "{remote_name}-milestone-{}-managed-change-{}",
        current.number,
        timestamp_slug(&current.remote_updated_at)
    );
    let directory = incoming_directory(root, remote_name, &current.remote_updated_at, &event_id)?;
    if directory.exists() {
        return Ok(0);
    }

    fs::create_dir_all(&directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    let event = MilestoneManagedChangeEvent {
        schema: MILESTONE_CHANGE_SCHEMA_V0.into(),
        id: event_id,
        remote: remote_name.into(),
        kind: "milestone.managed_fields.observed_change".into(),
        observed_at: current.observed_at.clone(),
        evidence: "GitHub REST milestone current-state observation changed since the previous local observation; the REST milestone representation does not identify the editing actor, so no actor is asserted.".into(),
        canonical_id: current.canonical_id.clone(),
        milestone_number: current.number,
        url: current.url.clone(),
        fields,
    };
    write_toml(directory.join("event.toml"), &event)?;
    write_toml(directory.join("before.toml"), &previous)?;
    fs::write(directory.join("before.description.md"), previous_description)
        .map_err(|error| format!("could not preserve previous GitHub milestone description: {error}"))?;
    write_toml(directory.join("after.toml"), current)?;
    fs::write(directory.join("after.description.md"), current_description)
        .map_err(|error| format!("could not preserve new GitHub milestone description: {error}"))?;
    Ok(1)
}

fn changed_fields(
    previous: &ObservedMilestone,
    previous_description: &str,
    current: &ObservedMilestone,
    current_description: &str,
) -> Vec<String> {
    let mut fields = Vec::new();
    if previous.title != current.title {
        fields.push("title".into());
    }
    if previous.state != current.state {
        fields.push("state".into());
    }
    if previous_description.trim_end() != current_description.trim_end() {
        fields.push("description".into());
    }
    if due_date(previous.due_on.as_deref()) != due_date(current.due_on.as_deref()) {
        fields.push("due".into());
    }
    fields
}

fn due_date(value: Option<&str>) -> Option<&str> {
    value.and_then(|value| value.get(..10))
}

fn same_remote_state(
    previous: &ObservedMilestone,
    previous_description: &str,
    current: &ObservedMilestone,
    current_description: &str,
) -> bool {
    previous.canonical_id == current.canonical_id
        && previous.number == current.number
        && previous.url == current.url
        && previous.title == current.title
        && previous.state == current.state
        && previous.due_on == current.due_on
        && previous.remote_updated_at == current.remote_updated_at
        && previous_description == current_description
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn canonical_due_date_projects_to_end_of_day_utc() {
        assert_eq!(
            canonical_due_on(Some("2026-12-01")),
            Some("2026-12-01T23:59:59Z".into())
        );
        assert_eq!(canonical_due_on(None), None);
    }

    #[test]
    fn due_time_representation_does_not_fake_canonical_drift() {
        let first = observed("M4", "open", Some("2026-12-01T00:00:00Z"), "r1");
        let second = observed("M4", "open", Some("2026-12-01T23:59:59Z"), "r2");
        assert!(changed_fields(&first, "Description\n", &second, "Description\n").is_empty());
    }

    #[test]
    fn observation_timestamp_is_not_remote_state() {
        let first = observed("M4", "open", None, "same-revision");
        let mut second = first.clone();
        second.observed_at = "2026-09-16T01:00:00Z".into();
        assert!(same_remote_state(
            &first,
            "Description",
            &second,
            "Description"
        ));
    }

    #[test]
    fn stale_milestone_update_sends_no_patch() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = Arc::clone(&requests);
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 8192];
            let count = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..count]);
            seen.lock()
                .unwrap()
                .push(request.lines().next().unwrap_or_default().to_string());
            let body = r#"{"number":7,"html_url":"https://example.invalid/milestone/7","title":"Externally changed","description":"Description","state":"open","due_on":null,"updated_at":"2026-09-16T01:00:00Z"}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-milestone-stale-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let previous = observed("Previously observed", "open", None, "2026-09-16T00:00:00Z");
        write_observed_milestone_snapshot(&root, "github", &previous, "Description").unwrap();
        let canonical = CanonicalMilestone {
            record: allodium_core::milestone::MilestoneRecord {
                schema: allodium_core::milestone::MILESTONE_SCHEMA_V0.into(),
                id: "milestone-0001".into(),
                title: "Canonical title".into(),
                state: "open".into(),
                due: None,
            },
            description: "Description".into(),
            directory: std::path::PathBuf::new(),
        };
        let adapter = GitHubAdapter {
            client: reqwest::blocking::Client::builder().build().unwrap(),
            repository: "owner/repo".into(),
            token: Some("test-token".into()),
            api_base: format!("http://{address}"),
        };

        let error = apply_update_milestone(
            &adapter,
            &root,
            "github",
            &canonical,
            7,
            &["title".into()],
        )
        .unwrap_err();
        assert!(error.contains("refusing stale update"), "{error}");
        server.join().unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].starts_with("GET "));
        fs::remove_dir_all(root).unwrap();
    }

    fn observed(
        title: &str,
        state: &str,
        due_on: Option<&str>,
        remote_updated_at: &str,
    ) -> ObservedMilestone {
        ObservedMilestone {
            schema: OBSERVED_MILESTONE_SCHEMA_V0.into(),
            canonical_id: "milestone-0001".into(),
            number: 7,
            url: "https://example.invalid/milestone/7".into(),
            title: title.into(),
            state: state.into(),
            due_on: due_on.map(str::to_owned),
            remote_updated_at: remote_updated_at.into(),
            observed_at: "2026-09-16T00:00:01Z".into(),
        }
    }
}
'''
Path('crates/allodium-github/src/milestone.rs').write_text(module)

lib = Path('crates/allodium-github/src/lib.rs')
text = lib.read_text()
if 'mod milestone;' not in text:
    text = text.replace('mod release;\n', 'mod milestone;\nmod release;\n', 1)
if 'use allodium_core::milestone::{CanonicalMilestone, load_milestones};' not in text:
    text = text.replace(
        'use allodium_core::release::{CanonicalRelease, load_releases};\n',
        'use allodium_core::milestone::{CanonicalMilestone, load_milestones};\nuse allodium_core::release::{CanonicalRelease, load_releases};\n',
        1,
    )
text = text.replace(
    '    pub release_managed_changes_archived: usize,\n',
    '    pub release_managed_changes_archived: usize,\n    pub milestones_observed: usize,\n    pub milestone_managed_changes_archived: usize,\n',
    1,
)
text = text.replace(
    '    pub releases_observed: usize,\n',
    '    pub releases_observed: usize,\n    pub milestones_created: usize,\n    pub milestones_updated: usize,\n    pub milestones_observed: usize,\n',
    1,
)
observe_anchor = '''        report.release_managed_changes_archived += release_report.managed_changes_archived;\n\n        for issue in self.fetch_repository_issues()? {\n'''
observe_replacement = '''        report.release_managed_changes_archived += release_report.managed_changes_archived;\n\n        let milestone_report = milestone::observe_milestones(self, root, remote_name)?;\n        report.milestones_observed += milestone_report.observed;\n        report.milestone_managed_changes_archived += milestone_report.managed_changes_archived;\n\n        for issue in self.fetch_repository_issues()? {\n'''
if 'milestone::observe_milestones' not in text:
    if observe_anchor not in text:
        raise SystemExit('observe milestone anchor not found')
    text = text.replace(observe_anchor, observe_replacement, 1)
load_anchor = '''        let releases = load_releases(root)?\n            .into_iter()\n            .map(|release| (release.record.id.clone(), release))\n            .collect::<BTreeMap<_, _>>();\n        let mut issue_mappings = load_mappings(root, &plan.remote)?;\n'''
load_replacement = '''        let releases = load_releases(root)?\n            .into_iter()\n            .map(|release| (release.record.id.clone(), release))\n            .collect::<BTreeMap<_, _>>();\n        let milestones = load_milestones(root)?\n            .into_iter()\n            .map(|milestone| (milestone.record.id.clone(), milestone))\n            .collect::<BTreeMap<_, _>>();\n        let mut issue_mappings = load_mappings(root, &plan.remote)?;\n'''
if 'let milestones = load_milestones(root)?' not in text:
    if load_anchor not in text:
        raise SystemExit('load milestone anchor not found')
    text = text.replace(load_anchor, load_replacement, 1)
match_anchor = '''                "update_release" => {\n                    let canonical = require_release(&releases, &operation.canonical_id)?;\n                    let release_id = require_number(operation)?;\n                    release::apply_update_release(\n                        self,\n                        root,\n                        &plan.remote,\n                        canonical,\n                        release_id,\n                        &operation.fields,\n                    )?;\n                    report.releases_updated += 1;\n                }\n                "observe_wiki" => {\n'''
match_replacement = '''                "update_release" => {\n                    let canonical = require_release(&releases, &operation.canonical_id)?;\n                    let release_id = require_number(operation)?;\n                    release::apply_update_release(\n                        self,\n                        root,\n                        &plan.remote,\n                        canonical,\n                        release_id,\n                        &operation.fields,\n                    )?;\n                    report.releases_updated += 1;\n                }\n                "create_milestone" => {\n                    let canonical = require_milestone(&milestones, &operation.canonical_id)?;\n                    milestone::apply_create_milestone(self, root, &plan.remote, canonical)?;\n                    report.milestones_created += 1;\n                }\n                "observe_milestone" => {\n                    let _canonical = require_milestone(&milestones, &operation.canonical_id)?;\n                    let number = require_number(operation)?;\n                    milestone::apply_observe_milestone(\n                        self,\n                        root,\n                        &plan.remote,\n                        &operation.canonical_id,\n                        number,\n                    )?;\n                    report.milestones_observed += 1;\n                }\n                "update_milestone" => {\n                    let canonical = require_milestone(&milestones, &operation.canonical_id)?;\n                    let number = require_number(operation)?;\n                    milestone::apply_update_milestone(\n                        self,\n                        root,\n                        &plan.remote,\n                        canonical,\n                        number,\n                        &operation.fields,\n                    )?;\n                    report.milestones_updated += 1;\n                }\n                "observe_wiki" => {\n'''
if '"create_milestone" =>' not in text:
    if match_anchor not in text:
        raise SystemExit('milestone apply anchor not found')
    text = text.replace(match_anchor, match_replacement, 1)
helper_anchor = '''fn github_token_from_environment() -> Option<String> {\n'''
helper = '''fn require_milestone<'a>(\n    milestones: &'a BTreeMap<String, CanonicalMilestone>,\n    canonical_id: &str,\n) -> Result<&'a CanonicalMilestone, String> {\n    milestones\n        .get(canonical_id)\n        .ok_or_else(|| format!("plan references unknown canonical milestone {canonical_id:?}"))\n}\n\n'''
if 'fn require_milestone' not in text:
    if helper_anchor not in text:
        raise SystemExit('milestone helper anchor not found')
    text = text.replace(helper_anchor, helper + helper_anchor, 1)
lib.write_text(text)
