from pathlib import Path

# Core planner: optional canonical description/color are unmanaged when absent.
path = Path("crates/allodium-core/src/github_label.rs")
text = path.read_text()
text = text.replace(
    'fields: vec!["name".into(), "description".into(), "color".into()],',
    'fields: managed_fields(label),',
)
old = '''    if observed.description != label.record.description {
        fields.push("description".into());
    }
    if observed.color != canonical_provider_color(label) {
        fields.push("color".into());
    }
'''
new = '''    if label.record.description.is_some() && observed.description != label.record.description {
        fields.push("description".into());
    }
    if let Some(color) = canonical_provider_color(label) {
        if !observed.color.eq_ignore_ascii_case(&color) {
            fields.push("color".into());
        }
    }
'''
if old not in text:
    raise SystemExit("label planner managed-field anchor not found")
text = text.replace(old, new, 1)
old = '''fn canonical_provider_color(label: &CanonicalLabel) -> String {
    label
        .record
        .color
        .as_deref()
        .unwrap_or("#ededed")
        .strip_prefix('#')
        .unwrap_or_else(|| label.record.color.as_deref().unwrap_or("ededed"))
        .to_ascii_lowercase()
}
'''
new = '''fn canonical_provider_color(label: &CanonicalLabel) -> Option<String> {
    label
        .record
        .color
        .as_deref()
        .map(|color| color.strip_prefix('#').unwrap_or(color).to_ascii_lowercase())
}

fn managed_fields(label: &CanonicalLabel) -> Vec<String> {
    let mut fields = vec!["name".into()];
    if label.record.description.is_some() {
        fields.push("description".into());
    }
    if label.record.color.is_some() {
        fields.push("color".into());
    }
    fields
}
'''
if old not in text:
    raise SystemExit("label planner color helper anchor not found")
text = text.replace(old, new, 1)
marker = '''    #[test]
    fn observation_with_different_provider_id_is_rejected() {
'''
test = '''    #[test]
    fn absent_optional_fields_are_unmanaged() {
        let root = test_project("optional-unmanaged", true);
        fs::write(
            root.join(".project/labels/label-allodium.toml"),
            "schema = \\"allodium.label/v0\\"\\nid = \\"label-allodium\\"\\nname = \\"allodium\\"\\n",
        )
        .unwrap();
        save_mapping(&root);
        save_observation(&root, "allodium", "ffffff", Some("Provider-owned description"));
        let plan = plan_labels(&root, "github").unwrap();
        assert!(plan.operations.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

'''
if "fn absent_optional_fields_are_unmanaged()" not in text:
    if marker not in text:
        raise SystemExit("label optional-field test anchor not found")
    text = text.replace(marker, test + marker, 1)
path.write_text(text)

# Runtime integration.
path = Path("crates/allodium-github/src/lib.rs")
text = path.read_text()
if "mod label;" not in text:
    text = text.replace("mod milestone;", "mod label;\nmod milestone;", 1)
if "use allodium_core::label::{CanonicalLabel, load_labels};" not in text:
    text = text.replace(
        "use allodium_core::milestone::{CanonicalMilestone, load_milestones};",
        "use allodium_core::label::{CanonicalLabel, load_labels};\nuse allodium_core::milestone::{CanonicalMilestone, load_milestones};",
        1,
    )
text = text.replace(
    "    pub milestone_managed_changes_archived: usize;\n",
    "    pub milestone_managed_changes_archived: usize;\n    pub labels_observed: usize;\n    pub label_managed_changes_archived: usize;\n",
    1,
)
text = text.replace(
    "    pub milestones_observed: usize;\n",
    "    pub milestones_observed: usize;\n    pub labels_created: usize;\n    pub labels_updated: usize;\n    pub labels_observed: usize;\n",
    1,
)
observe_anchor = '''        let milestone_report = milestone::observe_milestones(self, root, remote_name)?;
        report.milestones_observed += milestone_report.observed;
        report.milestone_managed_changes_archived += milestone_report.managed_changes_archived;
'''
observe_insert = observe_anchor + '''
        let label_report = label::observe_labels(self, root, remote_name)?;
        report.labels_observed += label_report.observed;
        report.label_managed_changes_archived += label_report.managed_changes_archived;
'''
if "label::observe_labels" not in text:
    if observe_anchor not in text:
        raise SystemExit("label observe integration anchor not found")
    text = text.replace(observe_anchor, observe_insert, 1)
map_anchor = '''        let milestones = load_milestones(root)?
            .into_iter()
            .map(|milestone| (milestone.record.id.clone(), milestone))
            .collect::<BTreeMap<_, _>>();
'''
map_insert = map_anchor + '''        let labels = load_labels(root)?
            .into_iter()
            .map(|label| (label.record.id.clone(), label))
            .collect::<BTreeMap<_, _>>();
'''
if "let labels = load_labels(root)?" not in text:
    if map_anchor not in text:
        raise SystemExit("label apply map anchor not found")
    text = text.replace(map_anchor, map_insert, 1)
action_anchor = '''                "observe_wiki" => {
'''
actions = '''                "create_label" => {
                    let canonical = require_label(&labels, &operation.canonical_id)?;
                    label::apply_create_label(self, root, &plan.remote, canonical)?;
                    report.labels_created += 1;
                }
                "observe_label" => {
                    let canonical = require_label(&labels, &operation.canonical_id)?;
                    let remote_id = require_number(operation)?;
                    label::apply_observe_label(
                        self,
                        root,
                        &plan.remote,
                        canonical,
                        remote_id,
                    )?;
                    report.labels_observed += 1;
                }
                "update_label" => {
                    let canonical = require_label(&labels, &operation.canonical_id)?;
                    let remote_id = require_number(operation)?;
                    label::apply_update_label(
                        self,
                        root,
                        &plan.remote,
                        canonical,
                        remote_id,
                        &operation.fields,
                    )?;
                    report.labels_updated += 1;
                }
'''
if '"create_label" =>' not in text:
    if action_anchor not in text:
        raise SystemExit("label action anchor not found")
    text = text.replace(action_anchor, actions + action_anchor, 1)
helper_anchor = '''fn require_milestone<'a>(
'''
helper = '''fn require_label<'a>(
    labels: &'a BTreeMap<String, CanonicalLabel>,
    canonical_id: &str,
) -> Result<&'a CanonicalLabel, String> {
    labels
        .get(canonical_id)
        .ok_or_else(|| format!("plan references unknown canonical label {canonical_id:?}"))
}

'''
if "fn require_label<'a>" not in text:
    if helper_anchor not in text:
        raise SystemExit("require label anchor not found")
    text = text.replace(helper_anchor, helper + helper_anchor, 1)
path.write_text(text)

# CLI activity reporting.
path = Path("crates/allodium-cli/src/main.rs")
text = path.read_text()
observe_anchor = '''            println!(
                "archived {} GitHub milestone managed-field remote change(s)",
                report.milestone_managed_changes_archived
            );
'''
observe_insert = observe_anchor + '''            println!("observed {} GitHub label(s)", report.labels_observed);
            println!(
                "archived {} GitHub label managed-field remote change(s)",
                report.label_managed_changes_archived
            );
'''
if "report.labels_observed" not in text:
    if observe_anchor not in text:
        raise SystemExit("label CLI observe anchor not found")
    text = text.replace(observe_anchor, observe_insert, 1)
apply_anchor = '''            println!(
                "observed {} GitHub milestone(s)",
                report.milestones_observed
            );
'''
apply_insert = apply_anchor + '''            println!("created {} GitHub label(s)", report.labels_created);
            println!("updated {} GitHub label(s)", report.labels_updated);
            println!("observed {} GitHub label(s)", report.labels_observed);
'''
if "report.labels_created" not in text:
    if apply_anchor not in text:
        raise SystemExit("label CLI apply anchor not found")
    text = text.replace(apply_anchor, apply_insert, 1)
path.write_text(text)
