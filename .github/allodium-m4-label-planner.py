from pathlib import Path

lib = Path("crates/allodium-core/src/lib.rs")
text = lib.read_text()
anchor = "pub mod github;\npub mod github_milestone;"
if "pub mod github_label;" not in text:
    if anchor not in text:
        raise SystemExit("github label module anchor not found")
    text = text.replace(anchor, "pub mod github;\npub mod github_label;\npub mod github_milestone;", 1)
lib.write_text(text)

cli = Path("crates/allodium-cli/src/main.rs")
text = cli.read_text()
anchor = "        plan.operations.extend(milestone_plan.operations);\n        Ok(plan)"
if "github_label::plan_labels" not in text:
    if anchor not in text:
        raise SystemExit("label planner CLI anchor not found")
    replacement = """        plan.operations.extend(milestone_plan.operations);

        let label_plan = allodium_core::github_label::plan_labels(&root, remote_name)?;
        if plan.remote != label_plan.remote || plan.repository != label_plan.repository {
            return Err("GitHub label projection plan disagrees about the configured remote".into());
        }
        plan.operations.extend(label_plan.operations);
        Ok(plan)"""
    text = text.replace(anchor, replacement, 1)
cli.write_text(text)
