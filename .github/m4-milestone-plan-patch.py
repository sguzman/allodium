from pathlib import Path

module = '''use crate::github::{GitHubOperation, GitHubPlan, PLAN_SCHEMA_V0};
use crate::load_remote;
use crate::milestone::{CanonicalMilestone, load_milestones};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const MILESTONE_MAPPINGS_SCHEMA_V0: &str = "allodium.github.milestone-mappings/v0";
pub const OBSERVED_MILESTONE_SCHEMA_V0: &str = "allodium.github.observed-milestone/v0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MilestoneMapping {
    pub number: u64,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MilestoneMappings {
    pub schema: String,
    #[serde(default)]
    pub milestones: BTreeMap<String, MilestoneMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservedMilestone {
    pub schema: String,
    pub canonical_id: String,
    pub number: u64,
    pub url: String,
    pub title: String,
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_on: Option<String>,
    pub remote_updated_at: String,
    pub observed_at: String,
}

pub fn plan_milestones(root: impl AsRef<Path>, remote_name: &str) -> Result<GitHubPlan, String> {
    let root = root.as_ref();
    let remote = load_remote(root, remote_name)?;
    if remote.kind != "github" {
        return Err(format!("remote {remote_name:?} has kind {:?}, not \\"github\\"", remote.kind));
    }
    let mappings = load_milestone_mappings(root, remote_name)?;
    let milestones = load_milestones(root)?;
    let mut operations = Vec::new();
    for milestone in milestones {
        plan_milestone(root, remote_name, &milestone, &mappings, &mut operations)?;
    }
    Ok(GitHubPlan { schema: PLAN_SCHEMA_V0.into(), remote: remote_name.into(), repository: remote.repository, operations })
}

pub fn load_milestone_mappings(root: impl AsRef<Path>, remote_name: &str) -> Result<MilestoneMappings, String> {
    let path = milestone_mappings_path(root.as_ref(), remote_name);
    if !path.exists() {
        return Ok(MilestoneMappings { schema: MILESTONE_MAPPINGS_SCHEMA_V0.into(), milestones: BTreeMap::new() });
    }
    let text = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mappings: MilestoneMappings = toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if mappings.schema != MILESTONE_MAPPINGS_SCHEMA_V0 {
        return Err(format!("{}: unsupported milestone mapping schema {:?}; expected {:?}", path.display(), mappings.schema, MILESTONE_MAPPINGS_SCHEMA_V0));
    }
    Ok(mappings)
}

pub fn save_milestone_mappings(root: impl AsRef<Path>, remote_name: &str, mappings: &MilestoneMappings) -> Result<(), String> {
    if mappings.schema != MILESTONE_MAPPINGS_SCHEMA_V0 {
        return Err(format!("refusing to write unsupported milestone mapping schema {:?}", mappings.schema));
    }
    let path = milestone_mappings_path(root.as_ref(), remote_name);
    fs::create_dir_all(path.parent().expect("milestone mapping path has parent")).map_err(|error| format!("{}: {error}", path.display()))?;
    let text = toml::to_string_pretty(mappings).map_err(|error| format!("could not serialize GitHub milestone mappings: {error}"))?;
    fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn load_observed_milestone(root: impl AsRef<Path>, remote_name: &str, canonical_id: &str) -> Result<Option<(ObservedMilestone, String)>, String> {
    let metadata_path = observed_milestone_path(root.as_ref(), remote_name, canonical_id);
    let description_path = observed_milestone_description_path(root.as_ref(), remote_name, canonical_id);
    if !metadata_path.exists() && !description_path.exists() { return Ok(None); }
    if !metadata_path.exists() || !description_path.exists() { return Err(format!("observed GitHub milestone snapshot for {canonical_id:?} is incomplete")); }
    let text = fs::read_to_string(&metadata_path).map_err(|error| format!("{}: {error}", metadata_path.display()))?;
    let observed: ObservedMilestone = toml::from_str(&text).map_err(|error| format!("{}: {error}", metadata_path.display()))?;
    if observed.schema != OBSERVED_MILESTONE_SCHEMA_V0 { return Err(format!("{}: unsupported observed milestone schema {:?}; expected {:?}", metadata_path.display(), observed.schema, OBSERVED_MILESTONE_SCHEMA_V0)); }
    let description = fs::read_to_string(&description_path).map_err(|error| format!("{}: {error}", description_path.display()))?;
    Ok(Some((observed, description)))
}

pub fn write_observed_milestone_snapshot(root: impl AsRef<Path>, remote_name: &str, observed: &ObservedMilestone, description: &str) -> Result<(), String> {
    if observed.schema != OBSERVED_MILESTONE_SCHEMA_V0 { return Err(format!("refusing to write unsupported observed milestone schema {:?}", observed.schema)); }
    let metadata_path = observed_milestone_path(root.as_ref(), remote_name, &observed.canonical_id);
    let description_path = observed_milestone_description_path(root.as_ref(), remote_name, &observed.canonical_id);
    fs::create_dir_all(metadata_path.parent().expect("observed milestone path has parent")).map_err(|error| format!("{}: {error}", metadata_path.display()))?;
    let text = toml::to_string_pretty(observed).map_err(|error| format!("could not serialize observed GitHub milestone: {error}"))?;
    fs::write(&metadata_path, text).map_err(|error| format!("{}: {error}", metadata_path.display()))?;
    fs::write(&description_path, description).map_err(|error| format!("{}: {error}", description_path.display()))
}

fn plan_milestone(root: &Path, remote_name: &str, milestone: &CanonicalMilestone, mappings: &MilestoneMappings, operations: &mut Vec<GitHubOperation>) -> Result<(), String> {
    let canonical_id = &milestone.record.id;
    let Some(mapping) = mappings.milestones.get(canonical_id) else {
        operations.push(GitHubOperation { canonical_id: canonical_id.clone(), action: "create_milestone".into(), number: None, fields: vec!["title".into(), "description".into(), "state".into(), "due".into()], reason: "canonical milestone has no GitHub milestone mapping".into() });
        return Ok(());
    };
    let Some((observed, observed_description)) = load_observed_milestone(root, remote_name, canonical_id)? else {
        operations.push(GitHubOperation { canonical_id: canonical_id.clone(), action: "observe_milestone".into(), number: Some(mapping.number), fields: vec!["title".into(), "description".into(), "state".into(), "due".into()], reason: "milestone mapping exists but the local observed GitHub milestone snapshot is missing".into() });
        return Ok(());
    };
    if observed.canonical_id != *canonical_id || observed.number != mapping.number { return Err(format!("observed GitHub milestone snapshot for {canonical_id:?} disagrees with its mapping")); }
    let mut fields = Vec::new();
    if observed.title != milestone.record.title { fields.push("title".into()); }
    if observed.state != milestone.record.state { fields.push("state".into()); }
    if observed_description.trim_end() != milestone.description.trim_end() { fields.push("description".into()); }
    if observed_due_date(observed.due_on.as_deref()) != milestone.record.due.as_deref() { fields.push("due".into()); }
    if !fields.is_empty() { operations.push(GitHubOperation { canonical_id: canonical_id.clone(), action: "update_milestone".into(), number: Some(mapping.number), fields, reason: "canonical managed milestone fields differ from the last observed GitHub milestone state".into() }); }
    Ok(())
}

fn observed_due_date(due_on: Option<&str>) -> Option<&str> { due_on.and_then(|value| value.get(..10)) }
fn milestone_mappings_path(root: &Path, remote_name: &str) -> PathBuf { root.join(".project/remotes").join(remote_name).join("mappings/milestones.toml") }
fn observed_milestone_path(root: &Path, remote_name: &str, canonical_id: &str) -> PathBuf { root.join(".project/remotes").join(remote_name).join("observed/milestones").join(format!("{canonical_id}.toml")) }
fn observed_milestone_description_path(root: &Path, remote_name: &str, canonical_id: &str) -> PathBuf { root.join(".project/remotes").join(remote_name).join("observed/milestones").join(format!("{canonical_id}.description.md")) }

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn unmapped_milestone_plans_create() {
        let root = test_project("create", None); let plan = plan_milestones(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1); assert_eq!(plan.operations[0].action, "create_milestone"); fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn mapped_milestone_without_observation_plans_observe() {
        let root = test_project("observe", None); save_mapping(&root); let plan = plan_milestones(&root, "github").unwrap();
        assert_eq!(plan.operations.len(), 1); assert_eq!(plan.operations[0].action, "observe_milestone"); fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn matching_milestone_is_idempotent() {
        let root = test_project("idempotent", Some("2026-12-01")); save_mapping(&root); save_observation(&root, "M4", "open", Some("2026-12-01T23:59:59Z"), "Description\n");
        let plan = plan_milestones(&root, "github").unwrap(); assert!(plan.operations.is_empty()); fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn mutable_drift_plans_update() {
        let root = test_project("update", Some("2026-12-01")); save_mapping(&root); save_observation(&root, "Old", "closed", Some("2026-11-30T00:00:00Z"), "Old description\n");
        let plan = plan_milestones(&root, "github").unwrap(); assert_eq!(plan.operations.len(), 1); assert_eq!(plan.operations[0].action, "update_milestone");
        for field in ["title", "state", "description", "due"] { assert!(plan.operations[0].fields.contains(&field.to_string())); }
        fs::remove_dir_all(root).unwrap();
    }

    fn test_project(name: &str, due: Option<&str>) -> PathBuf {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("allodium-github-milestone-{name}-{}-{nonce}", std::process::id()));
        let milestone_dir = root.join(".project/milestones/milestone-0001"); fs::create_dir_all(&milestone_dir).unwrap(); fs::create_dir_all(root.join(".project/remotes/github")).unwrap();
        fs::write(root.join(".project/remotes/github/remote.toml"), "schema = \"allodium.remote/v0\"\nname = \"github\"\nkind = \"github\"\nrepository = \"owner/repo\"\noutbound = \"reconcile\"\ninbound = \"archive\"\n").unwrap();
        let due = due.map(|due| format!("due = \"{due}\"\n")).unwrap_or_default();
        fs::write(milestone_dir.join("milestone.toml"), format!("schema = \"allodium.milestone/v0\"\nid = \"milestone-0001\"\ntitle = \"M4\"\nstate = \"open\"\n{due}")).unwrap();
        fs::write(milestone_dir.join("description.md"), "Description\n").unwrap(); root
    }
    fn save_mapping(root: &Path) {
        let mut mappings = MilestoneMappings { schema: MILESTONE_MAPPINGS_SCHEMA_V0.into(), milestones: BTreeMap::new() };
        mappings.milestones.insert("milestone-0001".into(), MilestoneMapping { number: 7, url: "https://example.invalid/milestones/7".into() }); save_milestone_mappings(root, "github", &mappings).unwrap();
    }
    fn save_observation(root: &Path, title: &str, state: &str, due_on: Option<&str>, description: &str) {
        let observed = ObservedMilestone { schema: OBSERVED_MILESTONE_SCHEMA_V0.into(), canonical_id: "milestone-0001".into(), number: 7, url: "https://example.invalid/milestones/7".into(), title: title.into(), state: state.into(), due_on: due_on.map(str::to_owned), remote_updated_at: "2026-09-16T00:00:00Z".into(), observed_at: "2026-09-16T00:00:01Z".into() };
        write_observed_milestone_snapshot(root, "github", &observed, description).unwrap();
    }
}
'''
Path('crates/allodium-core/src/github_milestone.rs').write_text(module)

lib = Path('crates/allodium-core/src/lib.rs')
text = lib.read_text()
needle = 'pub mod github_release;\n'
if 'pub mod github_milestone;' not in text:
    text = text.replace(needle, needle + 'pub mod github_milestone;\n', 1)
lib.write_text(text)

cli = Path('crates/allodium-cli/src/main.rs')
text = cli.read_text()
needle = '''        plan.operations.extend(release_plan.operations);\n        Ok(plan)\n'''
replacement = '''        plan.operations.extend(release_plan.operations);\n\n        let milestone_plan = allodium_core::github_milestone::plan_milestones(&root, remote_name)?;\n        if plan.remote != milestone_plan.remote || plan.repository != milestone_plan.repository {\n            return Err(\n                "GitHub milestone projection plan disagrees about the configured remote".into(),\n            );\n        }\n        plan.operations.extend(milestone_plan.operations);\n        Ok(plan)\n'''
if 'github_milestone::plan_milestones' not in text:
    if needle not in text:
        raise SystemExit('CLI plan anchor not found')
    text = text.replace(needle, replacement, 1)
cli.write_text(text)
