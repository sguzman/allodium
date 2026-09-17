from pathlib import Path
import re

path = Path("crates/allodium-github/src/project_runtime.rs")
text = path.read_text()
start = text.index("    fn managed_board_view_uses_verified_rest_identity_bridge_and_vertical_grouping() {")
end = text.index("    fn stale_project_state_refuses_update_before_mutation() {", start)
segment = text[start:end]
segment = re.sub(
    r'r#"(.*?)"#',
    lambda match: 'r#"' + match.group(1).replace('\\"', '"') + '"#',
    segment,
    flags=re.S,
)
path.write_text(text[:start] + segment + text[end:])
