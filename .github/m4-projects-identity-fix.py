from pathlib import Path

core = Path("crates/allodium-core/src/github_project.rs")
text = core.read_text()
old_import = "use std::collections::{BTreeMap, BTreeSet};"
if old_import not in text:
    raise SystemExit("expected github_project.rs import not found")
core.write_text(text.replace(old_import, "use std::collections::BTreeMap;", 1))

runtime = Path("crates/allodium-github/src/project.rs")
text = runtime.read_text()
old = r'[boards.board-0001]\nenabled = true\n"'
new = r'[boards.board-0001]\nenabled = true\ntarget = \"managed\"\n"'
if old not in text:
    raise SystemExit("expected Projects runtime test config fixture not found")
runtime.write_text(text.replace(old, new, 1))
