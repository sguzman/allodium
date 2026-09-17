from pathlib import Path
import json
import re

path = Path("crates/allodium-github/src/project_runtime.rs")
text = path.read_text()
start = text.index("    fn managed_board_view_uses_verified_rest_identity_bridge_and_vertical_grouping() {")
end = text.index("    fn stale_project_state_refuses_update_before_mutation() {", start)
segment = text[start:end]

pattern = re.compile(r'r#"(\{.*?\})"#', re.S)
seen = 0

def validate_or_repair(match: re.Match[str]) -> str:
    global seen
    seen += 1
    payload = match.group(1)
    try:
        json.loads(payload)
        return match.group(0)
    except json.JSONDecodeError as original:
        candidate = payload
        while candidate.endswith("}"):
            candidate = candidate[:-1]
            try:
                json.loads(candidate)
                return 'r#"' + candidate + '"#'
            except json.JSONDecodeError:
                pass
        raise SystemExit(f"malformed fake JSON response {seen}: {original}: {payload}")

segment = pattern.sub(validate_or_repair, segment)
if seen != 4:
    raise SystemExit(f"expected 4 fake JSON responses in ProjectV2 view test, found {seen}")
for index, match in enumerate(pattern.finditer(segment), start=1):
    json.loads(match.group(1))

path.write_text(text[:start] + segment + text[end:])
