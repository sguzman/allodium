from pathlib import Path

path = Path('crates/allodium-core/src/lib.rs')
text = path.read_text()

module_anchor = 'pub mod github_wiki;\npub mod wiki;\n'
module_replacement = 'pub mod github_wiki;\npub mod release;\npub mod wiki;\n'
if module_anchor not in text:
    raise SystemExit('missing core module anchor')
text = text.replace(module_anchor, module_replacement, 1)

validation_anchor = '    wiki::validate_wiki(root, &mut report);\n    validate_remotes(root, &mut report);\n'
validation_replacement = '    release::validate_releases(root, &mut report);\n    wiki::validate_wiki(root, &mut report);\n    validate_remotes(root, &mut report);\n'
if validation_anchor not in text:
    raise SystemExit('missing core validation anchor')
text = text.replace(validation_anchor, validation_replacement, 1)

path.write_text(text)
