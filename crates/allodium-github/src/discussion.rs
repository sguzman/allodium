use super::{GitHubAdapter, incoming_directory, now, write_toml};
use allodium_core::discussion::{CanonicalDiscussion, load_discussions};
use allodium_core::github_discussion::{
    DISCUSSION_MAPPINGS_SCHEMA_V0, DiscussionMapping, DiscussionMappings,
    OBSERVED_DISCUSSION_CAPABILITIES_SCHEMA_V0, OBSERVED_DISCUSSION_SCHEMA_V0, ObservedDiscussion,
    ObservedDiscussionCapabilities, ObservedDiscussionCategory, ObservedDiscussionSnapshot,
    load_discussion_mappings, load_discussion_projection_config, load_observed_discussion,
    load_observed_discussion_capabilities, save_discussion_mappings,
    write_observed_discussion_capabilities, write_observed_discussion_snapshot,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const DISCUSSION_CHANGE_SCHEMA_V0: &str =
    "allodium.github.discussion-managed-change-observation/v0";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct DiscussionObserveReport {
    pub capabilities_observed: usize,
    pub observed: usize,
    pub managed_changes_archived: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiRepositoryCapabilities {
    node_id: String,
    has_discussions: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphEnvelope<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Vec<GraphError>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphError {
    message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphCategoryData {
    repository: Option<GraphCategoryRepository>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphCategoryRepository {
    #[serde(rename = "discussionCategories")]
    discussion_categories: GraphCategoryConnection,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphCategoryConnection {
    nodes: Vec<GraphCategory>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphCategory {
    id: String,
    name: String,
    slug: String,
    #[serde(rename = "isAnswerable")]
    is_answerable: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphDiscussionData {
    repository: Option<GraphDiscussionRepository>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphDiscussionRepository {
    discussion: Option<GraphDiscussion>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphDiscussion {
    id: String,
    number: i64,
    url: String,
    title: String,
    body: String,
    closed: bool,
    #[serde(rename = "updatedAt")]
    updated_at: String,
    category: GraphDiscussionCategory,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphDiscussionCategory {
    id: String,
    name: String,
    slug: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphCreateData {
    #[serde(rename = "createDiscussion")]
    create_discussion: Option<GraphDiscussionPayload>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphUpdateData {
    #[serde(rename = "updateDiscussion")]
    update_discussion: Option<GraphDiscussionPayload>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphCloseData {
    #[serde(rename = "closeDiscussion")]
    close_discussion: Option<GraphDiscussionPayload>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphReopenData {
    #[serde(rename = "reopenDiscussion")]
    reopen_discussion: Option<GraphDiscussionPayload>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphDiscussionPayload {
    discussion: Option<GraphDiscussion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DiscussionManagedChangeEvent {
    schema: String,
    id: String,
    remote: String,
    kind: String,
    observed_at: String,
    evidence: String,
    canonical_id: String,
    number: u64,
    node_id: String,
    url: String,
    fields: Vec<String>,
}

pub(super) fn observe_discussions(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
) -> Result<DiscussionObserveReport, String> {
    let canonical = load_discussions(root)?
        .into_iter()
        .map(|discussion| (discussion.record.id.clone(), discussion))
        .collect::<BTreeMap<_, _>>();
    let mappings = load_discussion_mappings(root, remote_name)?;
    if canonical.is_empty() && mappings.discussions.is_empty() {
        return Ok(DiscussionObserveReport::default());
    }

    let capabilities = observe_capabilities(adapter, root, remote_name)?;
    let mut report = DiscussionObserveReport {
        capabilities_observed: 1,
        ..DiscussionObserveReport::default()
    };
    if !capabilities.enabled {
        return Ok(report);
    }

    for (canonical_id, mapping) in mappings.discussions {
        let live = fetch_discussion(adapter, mapping.number)?.ok_or_else(|| {
            format!(
                "mapped GitHub Discussion #{} for {canonical_id:?} is no longer observable; Discussion deletion/disappearance semantics are not defined in projection v0",
                mapping.number
            )
        })?;
        let snapshot = snapshot_from_graph(&canonical_id, &live, &now())?;
        assert_mapping_identity(&mapping, &snapshot)?;
        if canonical.contains_key(&canonical_id) {
            report.managed_changes_archived +=
                archive_managed_change_if_needed(root, remote_name, &snapshot)?;
        }
        write_observed_discussion_snapshot(root, remote_name, &snapshot)?;
        report.observed += 1;
    }

    Ok(report)
}

pub(super) fn apply_observe_discussion_capabilities(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
) -> Result<(), String> {
    observe_capabilities(adapter, root, remote_name).map(|_| ())
}

pub(super) fn apply_create_discussion(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
) -> Result<(), String> {
    adapter.require_write_token()?;
    let canonical = require_canonical(root, canonical_id)?;
    let config = load_discussion_projection_config(root, remote_name)?.ok_or_else(|| {
        "refusing GitHub Discussion creation: projection configuration is missing".to_string()
    })?;
    let binding = config.discussions.get(canonical_id).ok_or_else(|| {
        format!(
            "refusing GitHub Discussion creation: no category binding exists for {canonical_id:?}"
        )
    })?;
    let capabilities = load_observed_discussion_capabilities(root, remote_name)?.ok_or_else(|| {
        "refusing GitHub Discussion creation: capability observation is missing; observe and re-plan"
            .to_string()
    })?;
    if !capabilities.enabled {
        return Err(
            "refusing GitHub Discussion creation: Discussions are disabled; provider bootstrap is required"
                .into(),
        );
    }
    let category = capabilities
        .categories
        .iter()
        .find(|category| category.slug == binding.category_slug)
        .ok_or_else(|| {
            format!(
                "refusing GitHub Discussion creation: configured category slug {:?} is absent from the last observed provider category inventory",
                binding.category_slug
            )
        })?;

    let mut mappings = load_discussion_mappings(root, remote_name)?;
    if mappings.discussions.contains_key(canonical_id) {
        return Err(format!(
            "refusing create: canonical Discussion {canonical_id:?} already has a GitHub mapping"
        ));
    }

    let created = create_discussion(
        adapter,
        &capabilities.repository_node_id,
        &category.node_id,
        &canonical.record.title,
        &canonical.body,
    )?;
    let created_snapshot = snapshot_from_graph(canonical_id, &created, &now())?;
    let mapping = DiscussionMapping {
        number: created_snapshot.record.number,
        node_id: created_snapshot.record.node_id.clone(),
        url: created_snapshot.record.url.clone(),
        category_node_id: created_snapshot.record.category_node_id.clone(),
        category_slug: created_snapshot.record.category_slug.clone(),
    };
    mappings
        .discussions
        .insert(canonical_id.into(), mapping.clone());
    save_discussion_mappings(root, remote_name, &mappings)?;
    write_observed_discussion_snapshot(root, remote_name, &created_snapshot)?;

    if mapping.category_node_id != category.node_id
        || mapping.category_slug != binding.category_slug
    {
        return Err(format!(
            "GitHub created Discussion {canonical_id:?} in category {:?} ({}) instead of configured category {:?} ({}); the provider object has been mapped to prevent duplicate creation, but automatic reclassification is refused in v0",
            mapping.category_slug,
            mapping.category_node_id,
            binding.category_slug,
            category.node_id
        ));
    }

    if canonical.record.state == "closed" {
        let closed = mutate_discussion_state(adapter, &mapping.node_id, false)?;
        let closed_snapshot = snapshot_from_graph(canonical_id, &closed, &now())?;
        assert_mapping_identity(&mapping, &closed_snapshot)?;
        write_observed_discussion_snapshot(root, remote_name, &closed_snapshot)?;
    }
    Ok(())
}

pub(super) fn apply_observe_discussion(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    number: u64,
) -> Result<(), String> {
    let mapping = require_mapping(root, remote_name, canonical_id, number)?;
    let live = fetch_discussion(adapter, number)?.ok_or_else(|| {
        format!(
            "mapped GitHub Discussion #{number} for {canonical_id:?} is no longer observable; Discussion deletion/disappearance semantics are not defined in projection v0"
        )
    })?;
    let snapshot = snapshot_from_graph(canonical_id, &live, &now())?;
    assert_mapping_identity(&mapping, &snapshot)?;
    archive_managed_change_if_needed(root, remote_name, &snapshot)?;
    write_observed_discussion_snapshot(root, remote_name, &snapshot)
}

pub(super) fn apply_update_discussion(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    number: u64,
    fields: &[String],
) -> Result<(), String> {
    adapter.require_write_token()?;
    let canonical = require_canonical(root, canonical_id)?;
    let mapping = require_mapping(root, remote_name, canonical_id, number)?;
    assert_not_stale(adapter, root, remote_name, canonical_id, &mapping)?;
    let updated = update_discussion(adapter, &mapping.node_id, &canonical, fields)?;
    let snapshot = snapshot_from_graph(canonical_id, &updated, &now())?;
    assert_mapping_identity(&mapping, &snapshot)?;
    write_observed_discussion_snapshot(root, remote_name, &snapshot)
}

pub(super) fn apply_close_discussion(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    number: u64,
) -> Result<(), String> {
    apply_state_change(adapter, root, remote_name, canonical_id, number, false)
}

pub(super) fn apply_reopen_discussion(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    number: u64,
) -> Result<(), String> {
    apply_state_change(adapter, root, remote_name, canonical_id, number, true)
}

fn apply_state_change(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    number: u64,
    reopen: bool,
) -> Result<(), String> {
    adapter.require_write_token()?;
    let canonical = require_canonical(root, canonical_id)?;
    let expected = if reopen { "open" } else { "closed" };
    if canonical.record.state != expected {
        return Err(format!(
            "refusing GitHub Discussion state mutation: canonical {canonical_id:?} is {:?}, not {expected:?}",
            canonical.record.state
        ));
    }
    let mapping = require_mapping(root, remote_name, canonical_id, number)?;
    assert_not_stale(adapter, root, remote_name, canonical_id, &mapping)?;
    let changed = mutate_discussion_state(adapter, &mapping.node_id, reopen)?;
    let snapshot = snapshot_from_graph(canonical_id, &changed, &now())?;
    assert_mapping_identity(&mapping, &snapshot)?;
    write_observed_discussion_snapshot(root, remote_name, &snapshot)
}

fn observe_capabilities(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
) -> Result<ObservedDiscussionCapabilities, String> {
    let repository: ApiRepositoryCapabilities =
        adapter.get(&format!("/repos/{}", adapter.repository))?;
    let categories = if repository.has_discussions {
        fetch_categories(adapter)?
    } else {
        Vec::new()
    };
    let observed = ObservedDiscussionCapabilities {
        schema: OBSERVED_DISCUSSION_CAPABILITIES_SCHEMA_V0.into(),
        enabled: repository.has_discussions,
        repository_node_id: repository.node_id,
        categories,
        observed_at: now(),
    };
    write_observed_discussion_capabilities(root, remote_name, &observed)?;
    Ok(observed)
}

fn fetch_categories(adapter: &GitHubAdapter) -> Result<Vec<ObservedDiscussionCategory>, String> {
    let (owner, name) = repository_parts(adapter)?;
    let query = r#"
query DiscussionCategories($owner: String!, $name: String!) {
  repository(owner: $owner, name: $name) {
    discussionCategories(first: 100) {
      nodes { id name slug isAnswerable }
    }
  }
}
"#;
    let data: GraphCategoryData = graph_data(
        adapter,
        &json!({
            "query": query,
            "variables": { "owner": owner, "name": name }
        }),
        "Discussion-category query",
    )?;
    let mut categories = data
        .repository
        .map(|repository| repository.discussion_categories.nodes)
        .ok_or_else(|| {
            format!(
                "GitHub GraphQL returned no repository while reading Discussion categories for {}",
                adapter.repository
            )
        })?
        .into_iter()
        .map(|category| ObservedDiscussionCategory {
            node_id: category.id,
            name: category.name,
            slug: category.slug,
            is_answerable: category.is_answerable,
        })
        .collect::<Vec<_>>();
    categories.sort_by(|left, right| left.slug.cmp(&right.slug));
    Ok(categories)
}

fn fetch_discussion(
    adapter: &GitHubAdapter,
    number: u64,
) -> Result<Option<GraphDiscussion>, String> {
    let (owner, name) = repository_parts(adapter)?;
    let number = i32::try_from(number)
        .map_err(|_| format!("GitHub Discussion number {number} exceeds GraphQL Int range"))?;
    let query = r#"
query DiscussionByNumber($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    discussion(number: $number) {
      id number url title body closed updatedAt
      category { id name slug }
    }
  }
}
"#;
    let data: GraphDiscussionData = graph_data(
        adapter,
        &json!({
            "query": query,
            "variables": { "owner": owner, "name": name, "number": number }
        }),
        "Discussion query",
    )?;
    Ok(data.repository.and_then(|repository| repository.discussion))
}

fn create_discussion(
    adapter: &GitHubAdapter,
    repository_id: &str,
    category_id: &str,
    title: &str,
    body: &str,
) -> Result<GraphDiscussion, String> {
    let mutation = r#"
mutation CreateDiscussion($input: CreateDiscussionInput!) {
  createDiscussion(input: $input) {
    discussion {
      id number url title body closed updatedAt
      category { id name slug }
    }
  }
}
"#;
    let data: GraphCreateData = graph_data(
        adapter,
        &json!({
            "query": mutation,
            "variables": {
                "input": {
                    "repositoryId": repository_id,
                    "categoryId": category_id,
                    "title": title,
                    "body": body,
                }
            }
        }),
        "createDiscussion mutation",
    )?;
    data.create_discussion
        .and_then(|payload| payload.discussion)
        .ok_or_else(|| "GitHub createDiscussion returned no Discussion".into())
}

fn update_discussion(
    adapter: &GitHubAdapter,
    discussion_id: &str,
    canonical: &CanonicalDiscussion,
    fields: &[String],
) -> Result<GraphDiscussion, String> {
    let mut input = serde_json::Map::new();
    input.insert("discussionId".into(), json!(discussion_id));
    for field in fields {
        match field.as_str() {
            "title" => {
                input.insert("title".into(), json!(canonical.record.title));
            }
            "body" => {
                input.insert("body".into(), json!(canonical.body));
            }
            "category" => {
                return Err(
                    "GitHub Discussion projection v0 refuses automatic category changes after creation"
                        .into(),
                );
            }
            "state" => {
                return Err(
                    "GitHub Discussion state must use closeDiscussion/reopenDiscussion, not updateDiscussion"
                        .into(),
                );
            }
            other => {
                return Err(format!(
                    "GitHub Discussion projection v0 cannot update field {other:?}"
                ));
            }
        }
    }
    let mutation = r#"
mutation UpdateDiscussion($input: UpdateDiscussionInput!) {
  updateDiscussion(input: $input) {
    discussion {
      id number url title body closed updatedAt
      category { id name slug }
    }
  }
}
"#;
    let data: GraphUpdateData = graph_data(
        adapter,
        &json!({
            "query": mutation,
            "variables": { "input": serde_json::Value::Object(input) }
        }),
        "updateDiscussion mutation",
    )?;
    data.update_discussion
        .and_then(|payload| payload.discussion)
        .ok_or_else(|| "GitHub updateDiscussion returned no Discussion".into())
}

fn mutate_discussion_state(
    adapter: &GitHubAdapter,
    discussion_id: &str,
    reopen: bool,
) -> Result<GraphDiscussion, String> {
    if reopen {
        let mutation = r#"
mutation ReopenDiscussion($input: ReopenDiscussionInput!) {
  reopenDiscussion(input: $input) {
    discussion {
      id number url title body closed updatedAt
      category { id name slug }
    }
  }
}
"#;
        let data: GraphReopenData = graph_data(
            adapter,
            &json!({
                "query": mutation,
                "variables": { "input": { "discussionId": discussion_id } }
            }),
            "reopenDiscussion mutation",
        )?;
        data.reopen_discussion
            .and_then(|payload| payload.discussion)
            .ok_or_else(|| "GitHub reopenDiscussion returned no Discussion".into())
    } else {
        let mutation = r#"
mutation CloseDiscussion($input: CloseDiscussionInput!) {
  closeDiscussion(input: $input) {
    discussion {
      id number url title body closed updatedAt
      category { id name slug }
    }
  }
}
"#;
        let data: GraphCloseData = graph_data(
            adapter,
            &json!({
                "query": mutation,
                "variables": { "input": { "discussionId": discussion_id } }
            }),
            "closeDiscussion mutation",
        )?;
        data.close_discussion
            .and_then(|payload| payload.discussion)
            .ok_or_else(|| "GitHub closeDiscussion returned no Discussion".into())
    }
}

fn graph_data<T: DeserializeOwned>(
    adapter: &GitHubAdapter,
    payload: &serde_json::Value,
    context: &str,
) -> Result<T, String> {
    let envelope: GraphEnvelope<T> = adapter.post("/graphql", payload)?;
    if !envelope.errors.is_empty() {
        let messages = envelope
            .errors
            .into_iter()
            .map(|error| error.message)
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("GitHub GraphQL {context} failed: {messages}"));
    }
    envelope
        .data
        .ok_or_else(|| format!("GitHub GraphQL {context} returned no data"))
}

fn repository_parts(adapter: &GitHubAdapter) -> Result<(&str, &str), String> {
    adapter.repository.split_once('/').ok_or_else(|| {
        format!(
            "GitHub repository {:?} is not in owner/name form",
            adapter.repository
        )
    })
}

fn snapshot_from_graph(
    canonical_id: &str,
    discussion: &GraphDiscussion,
    observed_at: &str,
) -> Result<ObservedDiscussionSnapshot, String> {
    let number = u64::try_from(discussion.number).map_err(|_| {
        format!(
            "GitHub Discussion returned invalid negative number {}",
            discussion.number
        )
    })?;
    Ok(ObservedDiscussionSnapshot {
        record: ObservedDiscussion {
            schema: OBSERVED_DISCUSSION_SCHEMA_V0.into(),
            canonical_id: canonical_id.into(),
            number,
            node_id: discussion.id.clone(),
            url: discussion.url.clone(),
            title: discussion.title.clone(),
            state: if discussion.closed { "closed" } else { "open" }.into(),
            category_node_id: discussion.category.id.clone(),
            category_slug: discussion.category.slug.clone(),
            category_name: discussion.category.name.clone(),
            remote_updated_at: discussion.updated_at.clone(),
            observed_at: observed_at.into(),
        },
        body: discussion.body.clone(),
    })
}

fn require_canonical(root: &Path, canonical_id: &str) -> Result<CanonicalDiscussion, String> {
    load_discussions(root)?
        .into_iter()
        .find(|discussion| discussion.record.id == canonical_id)
        .ok_or_else(|| format!("plan references unknown canonical Discussion {canonical_id:?}"))
}

fn require_mapping(
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    number: u64,
) -> Result<DiscussionMapping, String> {
    let mappings = load_discussion_mappings(root, remote_name)?;
    let mapping = mappings.discussions.get(canonical_id).ok_or_else(|| {
        format!(
            "GitHub Discussion operation references {canonical_id:?}, but no provider mapping exists"
        )
    })?;
    if mapping.number != number {
        return Err(format!(
            "GitHub Discussion operation number {number} disagrees with mapped number {} for {canonical_id:?}",
            mapping.number
        ));
    }
    Ok(mapping.clone())
}

fn assert_mapping_identity(
    mapping: &DiscussionMapping,
    snapshot: &ObservedDiscussionSnapshot,
) -> Result<(), String> {
    if snapshot.record.number != mapping.number || snapshot.record.node_id != mapping.node_id {
        return Err(format!(
            "GitHub Discussion observation disagrees with its stable mapping: expected #{} / {}, observed #{} / {}",
            mapping.number, mapping.node_id, snapshot.record.number, snapshot.record.node_id
        ));
    }
    Ok(())
}

fn assert_not_stale(
    adapter: &GitHubAdapter,
    root: &Path,
    remote_name: &str,
    canonical_id: &str,
    mapping: &DiscussionMapping,
) -> Result<(), String> {
    let previous = load_observed_discussion(root, remote_name, canonical_id)?.ok_or_else(|| {
        format!(
            "refusing update of {canonical_id:?}: no observed GitHub Discussion snapshot; observe and re-plan before applying"
        )
    })?;
    assert_mapping_identity(mapping, &previous)?;
    let live = fetch_discussion(adapter, mapping.number)?.ok_or_else(|| {
        format!(
            "mapped GitHub Discussion #{} for {canonical_id:?} is no longer observable; refusing to guess whether it was deleted",
            mapping.number
        )
    })?;
    let current = snapshot_from_graph(canonical_id, &live, &now())?;
    assert_mapping_identity(mapping, &current)?;
    if previous.record.remote_updated_at != current.record.remote_updated_at
        || !same_root_state(&previous, &current)
    {
        return Err(format!(
            "refusing stale update of {canonical_id:?}: GitHub Discussion changed after the recorded observation; observe and re-plan before applying"
        ));
    }
    Ok(())
}

fn same_root_state(
    previous: &ObservedDiscussionSnapshot,
    current: &ObservedDiscussionSnapshot,
) -> bool {
    previous.record.number == current.record.number
        && previous.record.node_id == current.record.node_id
        && previous.record.title == current.record.title
        && previous.body == current.body
        && previous.record.state == current.record.state
        && previous.record.category_node_id == current.record.category_node_id
        && previous.record.category_slug == current.record.category_slug
}

fn archive_managed_change_if_needed(
    root: &Path,
    remote_name: &str,
    current: &ObservedDiscussionSnapshot,
) -> Result<usize, String> {
    let Some(previous) = load_observed_discussion(root, remote_name, &current.record.canonical_id)?
    else {
        return Ok(0);
    };
    let fields = changed_root_fields(&previous, current);
    if fields.is_empty() {
        return Ok(0);
    }
    let fingerprint = transition_fingerprint(&previous, current, &fields);
    let event_id = format!(
        "{remote_name}-discussion-{}-managed-change-{fingerprint}",
        current.record.number
    );
    let directory = incoming_directory(root, remote_name, &current.record.observed_at, &event_id)?;
    if directory.exists() {
        return Ok(0);
    }
    fs::create_dir_all(&directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    let event = DiscussionManagedChangeEvent {
        schema: DISCUSSION_CHANGE_SCHEMA_V0.into(),
        id: event_id,
        remote: remote_name.into(),
        kind: "discussion.managed_fields.observed_change".into(),
        observed_at: current.record.observed_at.clone(),
        evidence: "GitHub GraphQL Discussion observation changed since the previous local root snapshot. This observation does not prove which actor made the edit, so no actor is asserted. Category changes are archived as provider taxonomy drift and are not automatically repaired in v0.".into(),
        canonical_id: current.record.canonical_id.clone(),
        number: current.record.number,
        node_id: current.record.node_id.clone(),
        url: current.record.url.clone(),
        fields,
    };
    write_toml(directory.join("event.toml"), &event)?;
    write_toml(directory.join("before.toml"), &previous.record)?;
    fs::write(directory.join("before.body.md"), &previous.body)
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    write_toml(directory.join("after.toml"), &current.record)?;
    fs::write(directory.join("after.body.md"), &current.body)
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    Ok(1)
}

fn changed_root_fields(
    previous: &ObservedDiscussionSnapshot,
    current: &ObservedDiscussionSnapshot,
) -> Vec<String> {
    let mut fields = Vec::new();
    if previous.record.title != current.record.title {
        fields.push("title".into());
    }
    if previous.body != current.body {
        fields.push("body".into());
    }
    if previous.record.state != current.record.state {
        fields.push("state".into());
    }
    if previous.record.category_node_id != current.record.category_node_id
        || previous.record.category_slug != current.record.category_slug
    {
        fields.push("category".into());
    }
    fields
}

fn transition_fingerprint(
    previous: &ObservedDiscussionSnapshot,
    current: &ObservedDiscussionSnapshot,
    fields: &[String],
) -> String {
    let durable = format!(
        "{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{:?}",
        previous.record.canonical_id,
        previous.record.number,
        previous.record.title,
        previous.body,
        previous.record.state,
        previous.record.category_node_id,
        current.record.title,
        current.body,
        current.record.state,
        current.record.category_node_id,
        fields
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
    use allodium_core::discussion::{DISCUSSION_SCHEMA_V0, DiscussionRecord};
    use allodium_core::github_discussion::{
        DiscussionProjectionBinding, DiscussionProjectionConfig, write_observed_discussion_snapshot,
    };
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn disabled_repository_observation_never_queries_graphql() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = Arc::clone(&requests);
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            seen.lock().unwrap().push(request);
            respond(
                &mut stream,
                r#"{"node_id":"R_repo","has_discussions":false}"#,
            );
        });

        let root = test_root("disabled");
        write_canonical_discussion(&root, "open");
        let adapter = test_adapter(&format!("http://{address}"));
        let report = observe_discussions(&adapter, &root, "github").unwrap();
        assert_eq!(report.capabilities_observed, 1);
        assert_eq!(report.observed, 0);
        server.join().unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].starts_with("GET /repos/owner/repo "));
        let observed = load_observed_discussion_capabilities(&root, "github")
            .unwrap()
            .unwrap();
        assert!(!observed.enabled);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn enabled_repository_categories_are_sorted_and_persisted() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for body in [
                r#"{"node_id":"R_repo","has_discussions":true}"#,
                r#"{"data":{"repository":{"discussionCategories":{"nodes":[{"id":"DIC_z","name":"Zeta","slug":"zeta","isAnswerable":true},{"id":"DIC_a","name":"Alpha","slug":"alpha","isAnswerable":false}]}}}}"#,
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let _ = read_request(&mut stream);
                respond(&mut stream, body);
            }
        });
        let root = test_root("enabled");
        write_canonical_discussion(&root, "open");
        let adapter = test_adapter(&format!("http://{address}"));
        observe_discussions(&adapter, &root, "github").unwrap();
        server.join().unwrap();
        let observed = load_observed_discussion_capabilities(&root, "github")
            .unwrap()
            .unwrap();
        assert!(observed.enabled);
        assert_eq!(
            observed
                .categories
                .iter()
                .map(|category| category.slug.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha", "zeta"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_discussion_update_sends_no_mutation() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = Arc::clone(&requests);
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            seen.lock().unwrap().push(request);
            respond(
                &mut stream,
                &discussion_query_response(
                    "Provider changed title",
                    "Body\n",
                    false,
                    "2026-09-17T01:00:00Z",
                    "DIC_general",
                    "general",
                ),
            );
        });

        let root = test_root("stale");
        write_canonical_discussion(&root, "open");
        save_mapping(&root);
        write_observed_discussion_snapshot(
            &root,
            "github",
            &snapshot(
                "Previous title",
                "Body\n",
                "open",
                "2026-09-17T00:00:00Z",
                "DIC_general",
                "general",
            ),
        )
        .unwrap();
        let adapter = test_adapter(&format!("http://{address}"));
        let error = apply_update_discussion(
            &adapter,
            &root,
            "github",
            "discussion-0001",
            42,
            &["title".into()],
        )
        .unwrap_err();
        assert!(error.contains("refusing stale update"), "{error}");
        server.join().unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].contains("query DiscussionByNumber"));
        assert!(!requests[0].contains("mutation UpdateDiscussion"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn category_drift_is_archived_as_provider_taxonomy_change() {
        let root = test_root("category-drift");
        write_observed_discussion_snapshot(
            &root,
            "github",
            &snapshot(
                "Title",
                "Body\n",
                "open",
                "2026-09-17T00:00:00Z",
                "DIC_general",
                "general",
            ),
        )
        .unwrap();
        let current = snapshot(
            "Title",
            "Body\n",
            "open",
            "2026-09-17T01:00:00Z",
            "DIC_support",
            "support",
        );
        assert_eq!(
            archive_managed_change_if_needed(&root, "github", &current).unwrap(),
            1
        );
        let incoming = root.join(".project/remotes/github/incoming/2026/09");
        let event = fs::read_dir(incoming)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path()
            .join("event.toml");
        let text = fs::read_to_string(event).unwrap();
        assert!(text.contains("fields = [\"category\"]"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn update_payload_refuses_category_and_state() {
        let canonical = canonical("open");
        let adapter = test_adapter("http://127.0.0.1:9");
        for field in ["category", "state"] {
            let error = update_discussion(&adapter, "D_discussion", &canonical, &[field.into()])
                .unwrap_err();
            assert!(error.contains("refuses") || error.contains("must use"));
        }
    }

    fn canonical(state: &str) -> CanonicalDiscussion {
        CanonicalDiscussion {
            record: DiscussionRecord {
                schema: DISCUSSION_SCHEMA_V0.into(),
                id: "discussion-0001".into(),
                title: "Canonical title".into(),
                state: state.into(),
            },
            body: "Body\n".into(),
            directory: std::path::PathBuf::new(),
        }
    }

    fn write_canonical_discussion(root: &Path, state: &str) {
        let discussion = canonical(state);
        let directory = root.join(".project/discussions/discussion-0001");
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("discussion.toml"),
            format!(
                "schema = \"{}\"\nid = \"{}\"\ntitle = \"{}\"\nstate = \"{}\"\n",
                discussion.record.schema,
                discussion.record.id,
                discussion.record.title,
                discussion.record.state
            ),
        )
        .unwrap();
        fs::write(directory.join("body.md"), &discussion.body).unwrap();
        let remote = root.join(".project/remotes/github");
        fs::create_dir_all(&remote).unwrap();
        fs::write(
            remote.join("discussions.toml"),
            "schema = \"allodium.github.discussion-projection/v0\"\n\n[discussions.discussion-0001]\ncategory_slug = \"general\"\n",
        )
        .unwrap();
    }

    fn save_mapping(root: &Path) {
        let mut mappings = DiscussionMappings {
            schema: DISCUSSION_MAPPINGS_SCHEMA_V0.into(),
            discussions: BTreeMap::new(),
        };
        mappings.discussions.insert(
            "discussion-0001".into(),
            DiscussionMapping {
                number: 42,
                node_id: "D_discussion".into(),
                url: "https://github.com/owner/repo/discussions/42".into(),
                category_node_id: "DIC_general".into(),
                category_slug: "general".into(),
            },
        );
        save_discussion_mappings(root, "github", &mappings).unwrap();
    }

    fn snapshot(
        title: &str,
        body: &str,
        state: &str,
        updated_at: &str,
        category_node_id: &str,
        category_slug: &str,
    ) -> ObservedDiscussionSnapshot {
        ObservedDiscussionSnapshot {
            record: ObservedDiscussion {
                schema: OBSERVED_DISCUSSION_SCHEMA_V0.into(),
                canonical_id: "discussion-0001".into(),
                number: 42,
                node_id: "D_discussion".into(),
                url: "https://github.com/owner/repo/discussions/42".into(),
                title: title.into(),
                state: state.into(),
                category_node_id: category_node_id.into(),
                category_slug: category_slug.into(),
                category_name: category_slug.into(),
                remote_updated_at: updated_at.into(),
                observed_at: "2026-09-17T02:00:00Z".into(),
            },
            body: body.into(),
        }
    }

    fn discussion_query_response(
        title: &str,
        body: &str,
        closed: bool,
        updated_at: &str,
        category_id: &str,
        category_slug: &str,
    ) -> String {
        serde_json::to_string(&json!({
            "data": {
                "repository": {
                    "discussion": {
                        "id": "D_discussion",
                        "number": 42,
                        "url": "https://github.com/owner/repo/discussions/42",
                        "title": title,
                        "body": body,
                        "closed": closed,
                        "updatedAt": updated_at,
                        "category": {
                            "id": category_id,
                            "name": category_slug,
                            "slug": category_slug
                        }
                    }
                }
            }
        }))
        .unwrap()
    }

    fn read_request(stream: &mut std::net::TcpStream) -> String {
        let mut buffer = [0u8; 32768];
        let count = stream.read(&mut buffer).unwrap();
        String::from_utf8_lossy(&buffer[..count]).into_owned()
    }

    fn respond(stream: &mut std::net::TcpStream, body: &str) {
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    }

    fn test_root(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-discussion-runtime-{name}-{}-{nonce}",
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
