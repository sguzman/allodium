from pathlib import Path

path = Path('crates/allodium-github/src/milestone.rs')
text = path.read_text()

old = '''fn create_payload(milestone: &CanonicalMilestone) -> serde_json::Value {
    json!({
        "title": milestone.record.title,
        "state": milestone.record.state,
        "description": milestone.description,
        "due_on": canonical_due_on(milestone.record.due.as_deref()),
    })
}
'''
new = '''fn create_payload(milestone: &CanonicalMilestone) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    object.insert("title".into(), json!(milestone.record.title));
    object.insert("state".into(), json!(milestone.record.state));
    object.insert("description".into(), json!(milestone.description));
    if let Some(due_on) = canonical_due_on(milestone.record.due.as_deref()) {
        object.insert("due_on".into(), json!(due_on));
    }
    serde_json::Value::Object(object)
}
'''
if old not in text:
    raise SystemExit('create_payload anchor missing')
text = text.replace(old, new, 1)

old_due = '''            "due" => {
                object.insert(
                    "due_on".into(),
                    json!(canonical_due_on(milestone.record.due.as_deref())),
                );
            }
'''
new_due = '''            "due" => {
                let due_on = canonical_due_on(milestone.record.due.as_deref()).ok_or_else(|| {
                    format!(
                        "GitHub milestone projection v0 cannot clear an existing due date through the documented REST contract; canonical milestone {:?} has no due date",
                        milestone.record.id
                    )
                })?;
                object.insert("due_on".into(), json!(due_on));
            }
'''
if old_due not in text:
    raise SystemExit('update due anchor missing')
text = text.replace(old_due, new_due, 1)

test_anchor = '''    #[test]\n    fn due_time_representation_does_not_fake_canonical_drift() {\n'''
tests = '''    #[test]\n    fn create_without_due_omits_due_on_field() {\n        let milestone = CanonicalMilestone {\n            record: allodium_core::milestone::MilestoneRecord {\n                schema: allodium_core::milestone::MILESTONE_SCHEMA_V0.into(),\n                id: "milestone-0001".into(),\n                title: "M4".into(),\n                state: "open".into(),\n                due: None,\n            },\n            description: "Description".into(),\n            directory: std::path::PathBuf::new(),\n        };\n        let payload = create_payload(&milestone);\n        assert!(payload.get("due_on").is_none());\n    }\n\n    #[test]\n    fn update_refuses_undocumented_due_clear() {\n        let milestone = CanonicalMilestone {\n            record: allodium_core::milestone::MilestoneRecord {\n                schema: allodium_core::milestone::MILESTONE_SCHEMA_V0.into(),\n                id: "milestone-0001".into(),\n                title: "M4".into(),\n                state: "open".into(),\n                due: None,\n            },\n            description: "Description".into(),\n            directory: std::path::PathBuf::new(),\n        };\n        let error = update_payload(&milestone, &["due".into()]).unwrap_err();\n        assert!(error.contains("cannot clear an existing due date"), "{error}");\n    }\n\n'''
if test_anchor not in text:
    raise SystemExit('test anchor missing')
text = text.replace(test_anchor, tests + test_anchor, 1)
path.write_text(text)
