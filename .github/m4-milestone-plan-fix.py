from pathlib import Path

path = Path('crates/allodium-core/src/github_milestone.rs')
text = path.read_text()
text = text.replace(r'\\"github\\"', r'\"github\"')
path.write_text(text)
