from pathlib import Path

path = Path("crates/allodium-core/src/lib.rs")
text = path.read_text()

module_anchor = "pub mod github_wiki;\npub mod milestone;"
if "pub mod label;" not in text:
    if module_anchor not in text:
        raise SystemExit("label module anchor not found")
    text = text.replace(module_anchor, "pub mod github_wiki;\npub mod label;\npub mod milestone;", 1)

validation_anchor = "    milestone::validate_milestones(root, &mut report);"
if "label::validate_labels(root, &mut report);" not in text:
    if validation_anchor not in text:
        raise SystemExit("label validation anchor not found")
    text = text.replace(
        validation_anchor,
        "    label::validate_labels(root, &mut report);\n" + validation_anchor,
        1,
    )

path.write_text(text)
