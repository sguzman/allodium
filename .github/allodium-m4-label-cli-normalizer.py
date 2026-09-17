from pathlib import Path

path = Path("crates/allodium-cli/src/main.rs")
text = path.read_text()
marker = "fn github_apply(plan_path: PathBuf, root: PathBuf) -> ExitCode {"
if marker not in text:
    raise SystemExit("github_apply marker not found")
observe, apply = text.split(marker, 1)

for line in [
    '            println!("created {} GitHub label(s)", report.labels_created);\n',
    '            println!("updated {} GitHub label(s)", report.labels_updated);\n',
]:
    observe = observe.replace(line, "")
# The observe command legitimately keeps its observed-label counter.

if "report.labels_created" not in apply:
    anchor = '''            println!(
                "observed {} GitHub milestone(s)",
                report.milestones_observed
            );
'''
    if anchor not in apply:
        raise SystemExit("apply milestone output anchor not found")
    insert = anchor + '''            println!("created {} GitHub label(s)", report.labels_created);
            println!("updated {} GitHub label(s)", report.labels_updated);
            println!("observed {} GitHub label(s)", report.labels_observed);
'''
    apply = apply.replace(anchor, insert, 1)

path.write_text(observe + marker + apply)
