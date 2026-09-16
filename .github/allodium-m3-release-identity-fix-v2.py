from pathlib import Path
import runpy

# Apply the staged repair, then remove the duplicate tag/revision drift detector
# that the existing runtime already had before this repair was written.
runpy.run_path('.github/allodium-m3-release-identity-fix.py', run_name='__main__')

runtime = Path('crates/allodium-github/src/release.rs')
text = runtime.read_text()
old = '''    if previous.draft != current.draft {
        fields.push("state".into());
    }
    if previous.tag_name != current.tag_name {
        fields.push("tag".into());
    }
    if !previous.commit_sha.eq_ignore_ascii_case(&current.commit_sha) {
        fields.push("revision".into());
    }
'''
new = '''    if previous.draft != current.draft {
        fields.push("state".into());
    }
'''
if old not in text:
    raise SystemExit('missing duplicate release drift detector introduced by staged repair')
runtime.write_text(text.replace(old, new, 1))
