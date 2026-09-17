use super::{GitHubAdapter, incoming_directory, now, write_toml};
use allodium_core::github_label::{
    LabelMapping, OBSERVED_LABEL_SCHEMA_V0, ObservedLabel, load_label_mappings,
    load_observed_label, save_label_mappings, write_observed_label_snapshot,
};
use allodium_core::label::{CanonicalLabel, load_labels};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const LABEL_CHANGE_SCHEMA_V0: &str = "allodium.github.label-managed-change-observation/v0";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct LabelObserveReport {
    pub observed: usize,
    pub managed_changes_archived: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiLabel {
    id: u64,
    node_id: String,
    url: String,
    name: String,
    color: String,
    default: bool,
    description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LabelManagedChangeEvent {
    schema: String,
    id: String,
    remote: String,
    kind: String,
    observed_at: String,
    evidence: String,
    canonical_id: String,
    remote_id: u64,
    url: String,
    fields: Vec<String>,
}

pub(super) fn observe_labels(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
) -> Result<LabelObserveReport, String> {
    let mappings = load_label_mappings(root, remote_name)?;
    let canonical = load_labels(root)?
        .into_iter()
        .map(|label| (label.record.id.clone(), label))
        .collect::<BTreeMap<_, _>>();
    let live_labels = list_labels(adapter)?;
    let mut report = LabelObserveReport::default();

    for (canonical_id, mapping) in mappings.labels {
        let live = find_by_id(&live_labels, mapping.remote_id).ok_or_else(|| {
            format!(
                "mapped GitHub label id {} for {canonical_id:?} is no longer observable; label deletion/disappearance semantics are not defined in projection v0",
                mapping.remote_id
            )
        })?;
        let snapshot = snapshot_from_api(&canonical_id, live, &now());
        if let Some(label) = canonical.get(&canonical_id) {
            report.managed_changes_archived +=
                archive_managed_change_if_needed(root, remote_name, label, &snapshot)?;
        }
        write_observed_label_snapshot(root, remote_name, &snapshot)?;
        report.observed += 1;
    }

    Ok(report)
}

pub(super) fn apply_create_label(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    label: &CanonicalLabel,
) -> Result<(), String> {
    adapter.require_write_token()?;
    let mut mappings = load_label_mappings(root, remote_name)?;
    if mappings.labels.contains_key(&label.record.id) {
        return Err(format!(
            "refusing create: canonical label {:?} already has a GitHub label mapping",
            label.record.id
        ));
    }

    let live_labels = list_labels(adapter)?;
    reject_name_collision(&live_labels, &label.record.name, None)?;
    let created: ApiLabel = adapter.post(
        &format!("/repos/{}/labels", adapter.repository),
        &create_payload(label),
    )?;
    let snapshot = snapshot_from_api(&label.record.id, &created, &now());
    mappings.labels.insert(
        label.record.id.clone(),
        LabelMapping {
            remote_id: created.id,
        },
    );
    save_label_mappings(root, remote_name, &mappings)?;
    write_observed_label_snapshot(root, remote_name, &snapshot)
}

pub(super) fn apply_observe_label(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    label: &CanonicalLabel,
    remote_id: u64,
) -> Result<(), String> {
    let mappings = load_label_mappings(root, remote_name)?;
    let mapping = mappings.labels.get(&label.record.id).ok_or_else(|| {
        format!(
            "cannot observe GitHub label id {remote_id}: no mapping exists for {:?}",
            label.record.id
        )
    })?;
    if mapping.remote_id != remote_id {
        return Err(format!(
            "GitHub label operation id {remote_id} disagrees with mapping {} for {:?}",
            mapping.remote_id, label.record.id
        ));
    }

    let live_labels = list_labels(adapter)?;
    let live = find_by_id(&live_labels, remote_id).ok_or_else(|| {
        format!(
            "mapped GitHub label id {remote_id} for {:?} is no longer observable; label deletion/disappearance semantics are not defined in projection v0",
            label.record.id
        )
    })?;
    let snapshot = snapshot_from_api(&label.record.id, live, &now());
    archive_managed_change_if_needed(root, remote_name, label, &snapshot)?;
    write_observed_label_snapshot(root, remote_name, &snapshot)
}

pub(super) fn apply_update_label(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    label: &CanonicalLabel,
    remote_id: u64,
    fields: &[String],
) -> Result<(), String> {
    adapter.require_write_token()?;
    let mappings = load_label_mappings(root, remote_name)?;
    let mapping = mappings.labels.get(&label.record.id).ok_or_else(|| {
        format!(
            "refusing update of {:?}: no GitHub label mapping exists",
            label.record.id
        )
    })?;
    if mapping.remote_id != remote_id {
        return Err(format!(
            "refusing update of {:?}: plan label id {remote_id} disagrees with mapping {}",
            label.record.id, mapping.remote_id
        ));
    }

    let previous = load_observed_label(root, remote_name, &label.record.id)?.ok_or_else(|| {
        format!(
            "refusing update of {:?}: no observed GitHub label snapshot; observe and re-plan before applying",
            label.record.id
        )
    })?;
    if previous.remote_id != remote_id {
        return Err(format!(
            "refusing update of {:?}: observed provider id {} disagrees with mapped id {remote_id}",
            label.record.id, previous.remote_id
        ));
    }

    let live_labels = list_labels(adapter)?;
    let live = find_by_id(&live_labels, remote_id).ok_or_else(|| {
        format!(
            "mapped GitHub label id {remote_id} for {:?} is no longer observable; refusing to guess whether it was deleted",
            label.record.id
        )
    })?;
    let live_snapshot = snapshot_from_api(&label.record.id, live, &now());
    if !same_managed_state(&previous, &live_snapshot, label) {
        return Err(format!(
            "refusing stale update of {:?}: GitHub label managed state changed after the recorded observation; observe and re-plan before applying",
            label.record.id
        ));
    }

    if fields.iter().any(|field| field == "name") {
        reject_name_collision(&live_labels, &label.record.name, Some(remote_id))?;
    }
    let payload = update_payload(label, fields)?;
    let url = label_url(adapter, &live.name)?;
    let updated: ApiLabel =
        adapter.send(adapter.request(adapter.client.patch(url)).json(&payload))?;
    if updated.id != remote_id {
        return Err(format!(
            "GitHub label update for {:?} returned provider id {} instead of mapped id {remote_id}",
            label.record.id, updated.id
        ));
    }
    let snapshot = snapshot_from_api(&label.record.id, &updated, &now());
    write_observed_label_snapshot(root, remote_name, &snapshot)
}

fn list_labels(adapter: &GitHubAdapter) -> Result<Vec<ApiLabel>, String> {
    let mut page = 1;
    let mut labels = Vec::new();
    loop {
        let batch: Vec<ApiLabel> = adapter.get(&format!(
            "/repos/{}/labels?per_page=100&page={page}",
            adapter.repository
        ))?;
        let count = batch.len();
        labels.extend(batch);
        if count < 100 {
            break;
        }
        page += 1;
    }
    Ok(labels)
}

fn find_by_id(labels: &[ApiLabel], remote_id: u64) -> Option<&ApiLabel> {
    labels.iter().find(|label| label.id == remote_id)
}

fn reject_name_collision(
    labels: &[ApiLabel],
    target_name: &str,
    own_remote_id: Option<u64>,
) -> Result<(), String> {
    if let Some(conflict) = labels.iter().find(|label| {
        Some(label.id) != own_remote_id && label.name.eq_ignore_ascii_case(target_name)
    }) {
        return Err(format!(
            "refusing GitHub label projection: canonical target name {target_name:?} collides with foreign provider label id {} named {:?}; Allodium will not adopt or overwrite a label by display name",
            conflict.id, conflict.name
        ));
    }
    Ok(())
}

fn create_payload(label: &CanonicalLabel) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    object.insert("name".into(), json!(label.record.name));
    if let Some(description) = &label.record.description {
        object.insert("description".into(), json!(description));
    }
    if let Some(color) = canonical_color(label) {
        object.insert("color".into(), json!(color));
    }
    serde_json::Value::Object(object)
}

fn update_payload(label: &CanonicalLabel, fields: &[String]) -> Result<serde_json::Value, String> {
    let mut object = serde_json::Map::new();
    for field in fields {
        match field.as_str() {
            "name" => {
                object.insert("new_name".into(), json!(label.record.name));
            }
            "description" => {
                let description = label.record.description.as_ref().ok_or_else(|| {
                    format!(
                        "GitHub label projection v0 treats absent canonical description as unmanaged for {:?}",
                        label.record.id
                    )
                })?;
                object.insert("description".into(), json!(description));
            }
            "color" => {
                let color = canonical_color(label).ok_or_else(|| {
                    format!(
                        "GitHub label projection v0 treats absent canonical color as unmanaged for {:?}",
                        label.record.id
                    )
                })?;
                object.insert("color".into(), json!(color));
            }
            other => {
                return Err(format!(
                    "GitHub label projection v0 cannot update field {other:?}"
                ));
            }
        }
    }
    Ok(serde_json::Value::Object(object))
}

fn canonical_color(label: &CanonicalLabel) -> Option<String> {
    label.record.color.as_deref().map(|color| {
        color
            .strip_prefix('#')
            .unwrap_or(color)
            .to_ascii_lowercase()
    })
}

fn label_url(adapter: &GitHubAdapter, current_name: &str) -> Result<Url, String> {
    let mut url = Url::parse(&adapter.api_url(&format!("/repos/{}/labels", adapter.repository)))
        .map_err(|error| format!("could not build GitHub label URL: {error}"))?;
    url.path_segments_mut()
        .map_err(|_| "could not append GitHub label name to API URL".to_string())?
        .push(current_name);
    Ok(url)
}

fn snapshot_from_api(canonical_id: &str, label: &ApiLabel, observed_at: &str) -> ObservedLabel {
    ObservedLabel {
        schema: OBSERVED_LABEL_SCHEMA_V0.into(),
        canonical_id: canonical_id.into(),
        remote_id: label.id,
        node_id: label.node_id.clone(),
        url: label.url.clone(),
        name: label.name.clone(),
        color: label.color.to_ascii_lowercase(),
        description: label.description.clone(),
        default: label.default,
        observed_at: observed_at.into(),
    }
}

fn archive_managed_change_if_needed(
    root: &Path,
    remote_name: &str,
    canonical: &CanonicalLabel,
    current: &ObservedLabel,
) -> Result<usize, String> {
    let Some(previous) = load_observed_label(root, remote_name, &current.canonical_id)? else {
        return Ok(0);
    };
    let fields = changed_managed_fields(&previous, current, canonical);
    if fields.is_empty() {
        return Ok(0);
    }

    let fingerprint = transition_fingerprint(&previous, current, &fields);
    let event_id = format!(
        "{remote_name}-label-{}-managed-change-{fingerprint}",
        current.remote_id
    );
    let directory = incoming_directory(root, remote_name, &current.observed_at, &event_id)?;
    if directory.exists() {
        return Ok(0);
    }

    fs::create_dir_all(&directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    let event = LabelManagedChangeEvent {
        schema: LABEL_CHANGE_SCHEMA_V0.into(),
        id: event_id,
        remote: remote_name.into(),
        kind: "label.managed_fields.observed_change".into(),
        observed_at: current.observed_at.clone(),
        evidence: "GitHub REST label observation changed since the previous local managed snapshot. Labels expose no updated_at revision and this representation does not identify the editing actor, so no actor or edit time is asserted.".into(),
        canonical_id: current.canonical_id.clone(),
        remote_id: current.remote_id,
        url: current.url.clone(),
        fields,
    };
    write_toml(directory.join("event.toml"), &event)?;
    write_toml(directory.join("before.toml"), &previous)?;
    write_toml(directory.join("after.toml"), current)?;
    Ok(1)
}

fn changed_managed_fields(
    previous: &ObservedLabel,
    current: &ObservedLabel,
    canonical: &CanonicalLabel,
) -> Vec<String> {
    let mut fields = Vec::new();
    if previous.name != current.name {
        fields.push("name".into());
    }
    if canonical.record.description.is_some() && previous.description != current.description {
        fields.push("description".into());
    }
    if canonical.record.color.is_some() && !previous.color.eq_ignore_ascii_case(&current.color) {
        fields.push("color".into());
    }
    fields
}

fn same_managed_state(
    previous: &ObservedLabel,
    current: &ObservedLabel,
    canonical: &CanonicalLabel,
) -> bool {
    previous.remote_id == current.remote_id
        && changed_managed_fields(previous, current, canonical).is_empty()
}

fn transition_fingerprint(
    previous: &ObservedLabel,
    current: &ObservedLabel,
    fields: &[String],
) -> String {
    let durable = format!(
        "{}\0{}\0{}\0{:?}\0{}\0{}\0{:?}\0{}\0{}\0{:?}\0{:?}",
        previous.canonical_id,
        previous.remote_id,
        previous.name,
        previous.description,
        previous.color,
        current.name,
        current.description,
        current.color,
        current.remote_id,
        fields,
        current.default
    );
    let mut hash = 0xcbf29ce484222325u64;
    for byte in durable.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use allodium_core::github_label::{LABEL_MAPPINGS_SCHEMA_V0, LabelMappings};
    use allodium_core::label::{LABEL_SCHEMA_V0, LabelRecord};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn label_name_is_encoded_as_one_path_segment() {
        let adapter = test_adapter("http://127.0.0.1:1");
        let url = label_url(&adapter, "feature/a b").unwrap();
        assert!(url.as_str().ends_with("/labels/feature%2Fa%20b"));
    }

    #[test]
    fn absent_optional_fields_are_omitted_from_create_payload() {
        let label = canonical("allodium", None, None);
        let payload = create_payload(&label);
        assert!(payload.get("description").is_none());
        assert!(payload.get("color").is_none());
    }

    #[test]
    fn foreign_name_collision_is_rejected_by_provider_identity() {
        let labels = vec![api_label(77, "allodium")];
        let error = reject_name_collision(&labels, "ALLODIUM", None).unwrap_err();
        assert!(error.contains("foreign provider label id 77"));
        assert!(reject_name_collision(&labels, "allodium", Some(77)).is_ok());
    }

    #[test]
    fn observation_timestamp_and_provider_default_are_not_managed_state() {
        let canonical = canonical("allodium", Some("Test label"), Some("#6f42c1"));
        let first = observed("allodium", "6f42c1", Some("Test label"));
        let mut second = first.clone();
        second.observed_at = "2026-09-17T01:00:00Z".into();
        second.default = true;
        assert!(same_managed_state(&first, &second, &canonical));
    }

    #[test]
    fn stale_label_update_sends_no_patch() {
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
            let body = r#"[{"id":4242,"node_id":"LA_test","url":"https://example.invalid/labels/external","name":"externally-changed","color":"6f42c1","default":false,"description":"Test label"}]"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });

        let root = test_root("stale");
        let canonical = canonical("canonical-name", Some("Test label"), Some("#6f42c1"));
        let previous = observed("previously-observed", "6f42c1", Some("Test label"));
        write_observed_label_snapshot(&root, "github", &previous).unwrap();
        save_mapping(&root);
        let adapter = test_adapter(&format!("http://{address}"));

        let error = apply_update_label(
            &adapter,
            &root,
            "github",
            &canonical,
            4242,
            &["name".into()],
        )
        .unwrap_err();
        assert!(error.contains("refusing stale update"), "{error}");
        server.join().unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].starts_with("GET "));
        fs::remove_dir_all(root).unwrap();
    }

    fn canonical(name: &str, description: Option<&str>, color: Option<&str>) -> CanonicalLabel {
        CanonicalLabel {
            record: LabelRecord {
                schema: LABEL_SCHEMA_V0.into(),
                id: "label-allodium".into(),
                name: name.into(),
                description: description.map(str::to_owned),
                color: color.map(str::to_owned),
            },
            path: std::path::PathBuf::new(),
        }
    }

    fn api_label(id: u64, name: &str) -> ApiLabel {
        ApiLabel {
            id,
            node_id: "LA_test".into(),
            url: format!("https://example.invalid/labels/{name}"),
            name: name.into(),
            color: "6f42c1".into(),
            default: false,
            description: Some("Test label".into()),
        }
    }

    fn observed(name: &str, color: &str, description: Option<&str>) -> ObservedLabel {
        ObservedLabel {
            schema: OBSERVED_LABEL_SCHEMA_V0.into(),
            canonical_id: "label-allodium".into(),
            remote_id: 4242,
            node_id: "LA_test".into(),
            url: format!("https://example.invalid/labels/{name}"),
            name: name.into(),
            color: color.into(),
            description: description.map(str::to_owned),
            default: false,
            observed_at: "2026-09-17T00:00:00Z".into(),
        }
    }

    fn save_mapping(root: &Path) {
        let mut mappings = LabelMappings {
            schema: LABEL_MAPPINGS_SCHEMA_V0.into(),
            labels: BTreeMap::new(),
        };
        mappings
            .labels
            .insert("label-allodium".into(), LabelMapping { remote_id: 4242 });
        save_label_mappings(root, "github", &mappings).unwrap();
    }

    fn test_root(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-label-runtime-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn test_adapter(api_base: &str) -> GitHubAdapter {
        GitHubAdapter {
            client: reqwest::blocking::Client::builder().build().unwrap(),
            repository: "owner/repo".into(),
            token: Some("test-token".into()),
            api_base: api_base.into(),
        }
    }
}
