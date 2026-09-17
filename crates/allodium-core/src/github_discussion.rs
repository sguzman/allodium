use crate::discussion::{CanonicalDiscussion, load_discussions};
use crate::github::{GitHubOperation, GitHubPlan, PLAN_SCHEMA_V0};
use crate::load_remote;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const DISCUSSION_PROJECTION_SCHEMA_V0: &str = "allodium.github.discussion-projection/v0";
pub const DISCUSSION_MAPPINGS_SCHEMA_V0: &str = "allodium.github.discussion-mappings/v0";
pub const OBSERVED_DISCUSSION_CAPABILITIES_SCHEMA_V0: &str =
    "allodium.github.observed-discussion-capabilities/v0";
pub const OBSERVED_DISCUSSION_SCHEMA_V0: &str = "allodium.github.observed-discussion/v0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscussionProjectionBinding {
    pub category_slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscussionProjectionConfig {
    pub schema: String,
    #[serde(default)]
    pub discussions: BTreeMap<String, DiscussionProjectionBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscussionMapping {
    pub number: u64,
    pub node_id: String,
    pub url: String,
    pub category_node_id: String,
    pub category_slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscussionMappings {
    pub schema: String,
    #[serde(default)]
    pub discussions: BTreeMap<String, DiscussionMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedDiscussionCategory {
    pub node_id: String,
    pub name: String,
    pub slug: String,
    pub is_answerable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedDiscussionCapabilities {
    pub schema: String,
    pub enabled: bool,
    pub repository_node_id: String,
    #[serde(default)]
    pub categories: Vec<ObservedDiscussionCategory>,
    pub observed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedDiscussion {
    pub schema: String,
    pub canonical_id: String,
    pub number: u64,
    pub node_id: String,
    pub url: String,
    pub title: String,
    pub state: String,
    pub category_node_id: String,
    pub category_slug: String,
    pub category_name: String,
    pub remote_updated_at: String,
    pub observed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedDiscussionSnapshot {
    pub record: ObservedDiscussion,
    pub body: String,
}

pub fn plan_discussions(root: impl AsRef<Path>, remote_name: &str) -> Result<GitHubPlan, String> {
    let root = root.as_ref();
    let remote = load_remote(root, remote_name)?;
    if remote.kind != "github" {
        return Err(format!(
            "remote {remote_name:?} has kind {:?}, not \"github\"",
            remote.kind
        ));
    }

    let discussions = load_discussions(root)?;
    let mut operations = Vec::new();
    if discussions.is_empty() {
        return Ok(GitHubPlan {
            schema: PLAN_SCHEMA_V0.into(),
            remote: remote_name.into(),
            repository: remote.repository,
            operations,
        });
    }

    let Some(config) = load_discussion_projection_config(root, remote_name)? else {
        operations.push(GitHubOperation {
            canonical_id: "discussions".into(),
            action: "discussion_config_required".into(),
            number: None,
            fields: vec!["category_binding".into()],
            reason: "canonical discussions exist but the GitHub discussion projection configuration is missing".into(),
        });
        return Ok(plan(remote_name, remote.repository, operations));
    };

    let mut missing_binding = false;
    for discussion in &discussions {
        if !config.discussions.contains_key(&discussion.record.id) {
            missing_binding = true;
            operations.push(GitHubOperation {
                canonical_id: discussion.record.id.clone(),
                action: "discussion_config_required".into(),
                number: None,
                fields: vec!["category_binding".into()],
                reason: "canonical discussion has no explicit GitHub category binding".into(),
            });
        }
    }
    if missing_binding {
        return Ok(plan(remote_name, remote.repository, operations));
    }

    let Some(capabilities) = load_observed_discussion_capabilities(root, remote_name)? else {
        operations.push(GitHubOperation {
            canonical_id: "discussions".into(),
            action: "observe_discussion_capabilities".into(),
            number: None,
            fields: vec!["enabled".into(), "categories".into()],
            reason: "GitHub Discussion capability/category observation is missing".into(),
        });
        return Ok(plan(remote_name, remote.repository, operations));
    };

    if !capabilities.enabled {
        operations.push(GitHubOperation {
            canonical_id: "discussions".into(),
            action: "discussions_bootstrap_required".into(),
            number: None,
            fields: vec!["enabled".into()],
            reason: "GitHub Discussions are disabled for the configured repository; Allodium will not silently enable the provider feature".into(),
        });
        return Ok(plan(remote_name, remote.repository, operations));
    }

    let mappings = load_discussion_mappings(root, remote_name)?;
    for discussion in discussions {
        let binding = config
            .discussions
            .get(&discussion.record.id)
            .expect("bindings were checked above");
        plan_discussion(
            root,
            remote_name,
            &discussion,
            binding,
            &capabilities,
            &mappings,
            &mut operations,
        )?;
    }

    Ok(plan(remote_name, remote.repository, operations))
}

fn plan(remote_name: &str, repository: String, operations: Vec<GitHubOperation>) -> GitHubPlan {
    GitHubPlan {
        schema: PLAN_SCHEMA_V0.into(),
        remote: remote_name.into(),
        repository,
        operations,
    }
}

pub fn load_discussion_projection_config(
    root: impl AsRef<Path>,
    remote_name: &str,
) -> Result<Option<DiscussionProjectionConfig>, String> {
    let path = discussion_projection_config_path(root.as_ref(), remote_name);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let config: DiscussionProjectionConfig =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if config.schema != DISCUSSION_PROJECTION_SCHEMA_V0 {
        return Err(format!(
            "{}: unsupported GitHub discussion projection schema {:?}; expected {:?}",
            path.display(),
            config.schema,
            DISCUSSION_PROJECTION_SCHEMA_V0
        ));
    }
    for (canonical_id, binding) in &config.discussions {
        if canonical_id.trim().is_empty() || binding.category_slug.trim().is_empty() {
            return Err(format!(
                "{}: discussion bindings require non-empty canonical IDs and category_slug values",
                path.display()
            ));
        }
    }
    Ok(Some(config))
}

pub fn load_discussion_mappings(
    root: impl AsRef<Path>,
    remote_name: &str,
) -> Result<DiscussionMappings, String> {
    let path = discussion_mappings_path(root.as_ref(), remote_name);
    if !path.exists() {
        return Ok(DiscussionMappings {
            schema: DISCUSSION_MAPPINGS_SCHEMA_V0.into(),
            discussions: BTreeMap::new(),
        });
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mappings: DiscussionMappings =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if mappings.schema != DISCUSSION_MAPPINGS_SCHEMA_V0 {
        return Err(format!(
            "{}: unsupported GitHub discussion mapping schema {:?}; expected {:?}",
            path.display(),
            mappings.schema,
            DISCUSSION_MAPPINGS_SCHEMA_V0
        ));
    }
    Ok(mappings)
}

pub fn save_discussion_mappings(
    root: impl AsRef<Path>,
    remote_name: &str,
    mappings: &DiscussionMappings,
) -> Result<(), String> {
    if mappings.schema != DISCUSSION_MAPPINGS_SCHEMA_V0 {
        return Err(format!(
            "refusing to write unsupported GitHub discussion mapping schema {:?}",
            mappings.schema
        ));
    }
    let path = discussion_mappings_path(root.as_ref(), remote_name);
    fs::create_dir_all(path.parent().expect("discussion mapping path has parent"))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let text = toml::to_string_pretty(mappings)
        .map_err(|error| format!("could not serialize GitHub discussion mappings: {error}"))?;
    fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn load_observed_discussion_capabilities(
    root: impl AsRef<Path>,
    remote_name: &str,
) -> Result<Option<ObservedDiscussionCapabilities>, String> {
    let path = observed_discussion_capabilities_path(root.as_ref(), remote_name);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let observed: ObservedDiscussionCapabilities =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if observed.schema != OBSERVED_DISCUSSION_CAPABILITIES_SCHEMA_V0 {
        return Err(format!(
            "{}: unsupported observed GitHub Discussion capability schema {:?}; expected {:?}",
            path.display(),
            observed.schema,
            OBSERVED_DISCUSSION_CAPABILITIES_SCHEMA_V0
        ));
    }
    Ok(Some(observed))
}

pub fn write_observed_discussion_capabilities(
    root: impl AsRef<Path>,
    remote_name: &str,
    observed: &ObservedDiscussionCapabilities,
) -> Result<(), String> {
    if observed.schema != OBSERVED_DISCUSSION_CAPABILITIES_SCHEMA_V0 {
        return Err(format!(
            "refusing to write unsupported observed GitHub Discussion capability schema {:?}",
            observed.schema
        ));
    }
    let path = observed_discussion_capabilities_path(root.as_ref(), remote_name);
    fs::create_dir_all(path.parent().expect("discussion capability path has parent"))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let text = toml::to_string_pretty(observed)
        .map_err(|error| format!("could not serialize GitHub Discussion capabilities: {error}"))?;
    fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn load_observed_discussion(
    root: impl AsRef<Path>,
    remote_name: &str,
    canonical_id: &str,
) -> Result<Option<ObservedDiscussionSnapshot>, String> {
    let record_path = observed_discussion_path(root.as_ref(), remote_name, canonical_id);
    if !record_path.exists() {
        return Ok(None);
    }
    let body_path = observed_discussion_body_path(root.as_ref(), remote_name, canonical_id);
    let text = fs::read_to_string(&record_path)
        .map_err(|error| format!("{}: {error}", record_path.display()))?;
    let record: ObservedDiscussion =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", record_path.display()))?;
    if record.schema != OBSERVED_DISCUSSION_SCHEMA_V0 {
        return Err(format!(
            "{}: unsupported observed GitHub Discussion schema {:?}; expected {:?}",
            record_path.display(),
            record.schema,
            OBSERVED_DISCUSSION_SCHEMA_V0
        ));
    }
    let body =
        fs::read_to_string(&body_path).map_err(|error| format!("{}: {error}", body_path.display()))?;
    Ok(Some(ObservedDiscussionSnapshot { record, body }))
}

pub fn write_observed_discussion_snapshot(
    root: impl AsRef<Path>,
    remote_name: &str,
    snapshot: &ObservedDiscussionSnapshot,
) -> Result<(), String> {
    if snapshot.record.schema != OBSERVED_DISCUSSION_SCHEMA_V0 {
        return Err(format!(
            "refusing to write unsupported observed GitHub Discussion schema {:?}",
            snapshot.record.schema
        ));
    }
    let record_path = observed_discussion_path(root.as_ref(), remote_name, &snapshot.record.canonical_id);
    let body_path = observed_discussion_body_path(root.as_ref(), remote_name, &snapshot.record.canonical_id);
    fs::create_dir_all(record_path.parent().expect("observed discussion path has parent"))
        .map_err(|error| format!("{}: {error}", record_path.display()))?;
    let text = toml::to_string_pretty(&snapshot.record)
        .map_err(|error| format!("could not serialize observed GitHub Discussion: {error}"))?;
    fs::write(&record_path, text).map_err(|error| format!("{}: {error}", record_path.display()))?;
    fs::write(&body_path, &snapshot.body)
        .map_err(|error| format!("{}: {error}", body_path.display()))
}

fn plan_discussion(
    root: &Path,
    remote_name: &str,
    discussion: &CanonicalDiscussion,
    binding: &DiscussionProjectionBinding,
    capabilities: &ObservedDiscussionCapabilities,
    mappings: &DiscussionMappings,
    operations: &mut Vec<GitHubOperation>,
) -> Result<(), String> {
    let canonical_id = &discussion.record.id;
    let Some(mapping) = mappings.discussions.get(canonical_id) else {
        if capabilities
            .categories
            .iter()
            .all(|category| category.slug != binding.category_slug)
        {
            operations.push(GitHubOperation {
                canonical_id: canonical_id.clone(),
                action: "discussion_category_required".into(),
                number: None,
                fields: vec!["category".into()],
                reason: format!(
                    "configured GitHub Discussion category slug {:?} is not present in the last observed provider category inventory; Allodium will not guess a category",
                    binding.category_slug
                ),
            });
            return Ok(());
        }

        operations.push(GitHubOperation {
            canonical_id: canonical_id.clone(),
            action: "create_discussion".into(),
            number: None,
            fields: vec!["title".into(), "body".into(), "state".into(), "category".into()],
            reason: "canonical discussion has no GitHub Discussion mapping".into(),
        });
        return Ok(());
    };

    if mapping.category_slug != binding.category_slug {
        operations.push(GitHubOperation {
            canonical_id: canonical_id.clone(),
            action: "discussion_category_change_unsupported".into(),
            number: Some(mapping.number),
            fields: vec!["category".into()],
            reason: format!(
                "mapped Discussion was created with category binding {:?}, but configuration now requests {:?}; v0 refuses to reclassify an existing provider conversation",
                mapping.category_slug, binding.category_slug
            ),
        });
        return Ok(());
    }

    let Some(observed) = load_observed_discussion(root, remote_name, canonical_id)? else {
        operations.push(GitHubOperation {
            canonical_id: canonical_id.clone(),
            action: "observe_discussion".into(),
            number: Some(mapping.number),
            fields: vec!["title".into(), "body".into(), "state".into(), "category".into()],
            reason: "Discussion mapping exists but the local observed GitHub Discussion snapshot is missing".into(),
        });
        return Ok(());
    };

    if observed.record.canonical_id != *canonical_id
        || observed.record.number != mapping.number
        || observed.record.node_id != mapping.node_id
    {
        return Err(format!(
            "observed GitHub Discussion snapshot for {canonical_id:?} disagrees with its stable provider mapping"
        ));
    }

    if observed.record.category_node_id != mapping.category_node_id {
        operations.push(GitHubOperation {
            canonical_id: canonical_id.clone(),
            action: "discussion_category_drift_requires_review".into(),
            number: Some(mapping.number),
            fields: vec!["category".into()],
            reason: "the provider Discussion moved to a different category than its frozen v0 projection binding; category is provider taxonomy, so Allodium will archive the drift but will not silently move it back".into(),
        });
        return Ok(());
    }

    let mut mutable_fields = Vec::new();
    if observed.record.title != discussion.record.title {
        mutable_fields.push("title".into());
    }
    if observed.body != discussion.body {
        mutable_fields.push("body".into());
    }
    if !mutable_fields.is_empty() {
        operations.push(GitHubOperation {
            canonical_id: canonical_id.clone(),
            action: "update_discussion".into(),
            number: Some(mapping.number),
            fields: mutable_fields,
            reason: "canonical managed Discussion fields differ from the last observed GitHub Discussion state".into(),
        });
    }

    if observed.record.state != discussion.record.state {
        let action = match discussion.record.state.as_str() {
            "open" => "reopen_discussion",
            "closed" => "close_discussion",
            _ => unreachable!("canonical discussion state is validated before planning"),
        };
        operations.push(GitHubOperation {
            canonical_id: canonical_id.clone(),
            action: action.into(),
            number: Some(mapping.number),
            fields: vec!["state".into()],
            reason: "canonical Discussion state differs from the last observed GitHub Discussion state".into(),
        });
    }

    Ok(())
}

fn discussion_projection_config_path(root: &Path, remote_name: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("discussions.toml")
}

fn discussion_mappings_path(root: &Path, remote_name: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("mappings/discussions.toml")
}

fn observed_discussion_capabilities_path(root: &Path, remote_name: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("observed/discussions/capabilities.toml")
}

fn observed_discussion_path(root: &Path, remote_name: &str, canonical_id: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("observed/discussions")
        .join(format!("{canonical_id}.toml"))
}

fn observed_discussion_body_path(root: &Path, remote_name: &str, canonical_id: &str) -> PathBuf {
    root.join(".project/remotes")
        .join(remote_name)
        .join("observed/discussions")
        .join(format!("{canonical_id}.body.md"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn missing_projection_config_is_non_mutating_requirement() {
        let root = test_project("missing-config", true, false);
        let plan = plan_discussions(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "discussion_config_required");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_capability_observation_plans_observation() {
        let root = test_project("observe-capabilities", true, true);
        let plan = plan_discussions(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "observe_discussion_capabilities");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disabled_discussions_are_bootstrap_requirement() {
        let root = test_project("disabled", true, true);
        save_capabilities(&root, false, &[]);
        let plan = plan_discussions(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "discussions_bootstrap_required");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_bound_category_never_guesses() {
        let root = test_project("missing-category", true, true);
        save_capabilities(&root, true, &[category("other", "Other", "DIC_other")]);
        let plan = plan_discussions(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "discussion_category_required");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unmapped_discussion_plans_create_after_capability_resolution() {
        let root = test_project("create", true, true);
        save_capabilities(&root, true, &[category("general", "General", "DIC_general")]);
        let plan = plan_discussions(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "create_discussion");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mapped_discussion_without_observation_plans_observe() {
        let root = test_project("observe", true, true);
        save_capabilities(&root, true, &[category("general", "General", "DIC_general")]);
        save_mapping(&root, "general", "DIC_general");
        let plan = plan_discussions(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(plan.operations[0].action, "observe_discussion");
        assert_eq!(plan.operations[0].number, Some(42));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn matching_discussion_is_idempotent() {
        let root = test_project("idempotent", true, true);
        save_capabilities(&root, true, &[category("general", "General", "DIC_general")]);
        save_mapping(&root, "general", "DIC_general");
        save_observation(&root, "Allodium project discussion", "Discussion body\n", "open", "DIC_general");
        let plan = plan_discussions(&root, "github").unwrap();
        assert!(plan.operations.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn canonical_fields_and_state_plan_separate_mutations() {
        let root = test_project("drift", true, true);
        save_capabilities(&root, true, &[category("general", "General", "DIC_general")]);
        save_mapping(&root, "general", "DIC_general");
        save_observation(&root, "Provider title", "Provider body\n", "closed", "DIC_general");
        let plan = plan_discussions(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 2);
        assert_eq!(plan.operations[0].action, "update_discussion");
        assert_eq!(plan.operations[1].action, "reopen_discussion");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn configuration_cannot_reclassify_mapped_discussion() {
        let root = test_project("config-reclassify", true, true);
        save_capabilities(&root, true, &[category("general", "General", "DIC_general")]);
        save_mapping(&root, "support", "DIC_general");
        let plan = plan_discussions(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(
            plan.operations[0].action,
            "discussion_category_change_unsupported"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn provider_category_drift_blocks_automatic_repair() {
        let root = test_project("category-drift", true, true);
        save_capabilities(&root, true, &[category("general", "General", "DIC_general")]);
        save_mapping(&root, "general", "DIC_general");
        save_observation(&root, "Allodium project discussion", "Discussion body\n", "open", "DIC_other");
        let plan = plan_discussions(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1);
        assert_eq!(
            plan.operations[0].action,
            "discussion_category_drift_requires_review"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn absent_canonical_discussion_does_not_plan_delete() {
        let root = test_project("no-delete", false, true);
        save_mapping(&root, "general", "DIC_general");
        let plan = plan_discussions(&root, "github").unwrap();
        assert!(plan.operations.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    fn test_project(name: &str, with_discussion: bool, with_config: bool) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "allodium-github-discussion-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join(".project/remotes/github")).unwrap();
        fs::write(
            root.join(".project/remotes/github/remote.toml"),
            "schema = \"allodium.remote/v0\"\nname = \"github\"\nkind = \"github\"\nrepository = \"owner/repo\"\noutbound = \"reconcile\"\ninbound = \"archive\"\n",
        )
        .unwrap();
        if with_discussion {
            let directory = root.join(".project/discussions/discussion-0001");
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                directory.join("discussion.toml"),
                "schema = \"allodium.discussion/v0\"\nid = \"discussion-0001\"\ntitle = \"Allodium project discussion\"\nstate = \"open\"\n",
            )
            .unwrap();
            fs::write(directory.join("body.md"), "Discussion body\n").unwrap();
        }
        if with_config {
            fs::write(
                root.join(".project/remotes/github/discussions.toml"),
                "schema = \"allodium.github.discussion-projection/v0\"\n\n[discussions.discussion-0001]\ncategory_slug = \"general\"\n",
            )
            .unwrap();
        }
        root
    }

    fn category(slug: &str, name: &str, node_id: &str) -> ObservedDiscussionCategory {
        ObservedDiscussionCategory {
            node_id: node_id.into(),
            name: name.into(),
            slug: slug.into(),
            is_answerable: false,
        }
    }

    fn save_capabilities(root: &Path, enabled: bool, categories: &[ObservedDiscussionCategory]) {
        write_observed_discussion_capabilities(
            root,
            "github",
            &ObservedDiscussionCapabilities {
                schema: OBSERVED_DISCUSSION_CAPABILITIES_SCHEMA_V0.into(),
                enabled,
                repository_node_id: "R_repo".into(),
                categories: categories.to_vec(),
                observed_at: "2026-09-17T00:00:00Z".into(),
            },
        )
        .unwrap();
    }

    fn save_mapping(root: &Path, category_slug: &str, category_node_id: &str) {
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
                category_node_id: category_node_id.into(),
                category_slug: category_slug.into(),
            },
        );
        save_discussion_mappings(root, "github", &mappings).unwrap();
    }

    fn save_observation(root: &Path, title: &str, body: &str, state: &str, category_node_id: &str) {
        write_observed_discussion_snapshot(
            root,
            "github",
            &ObservedDiscussionSnapshot {
                record: ObservedDiscussion {
                    schema: OBSERVED_DISCUSSION_SCHEMA_V0.into(),
                    canonical_id: "discussion-0001".into(),
                    number: 42,
                    node_id: "D_discussion".into(),
                    url: "https://github.com/owner/repo/discussions/42".into(),
                    title: title.into(),
                    state: state.into(),
                    category_node_id: category_node_id.into(),
                    category_slug: "general".into(),
                    category_name: "General".into(),
                    remote_updated_at: "2026-09-17T00:00:00Z".into(),
                    observed_at: "2026-09-17T00:00:01Z".into(),
                },
                body: body.into(),
            },
        )
        .unwrap();
    }
}
