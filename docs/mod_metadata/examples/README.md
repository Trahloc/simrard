# Mod manifest examples

- **mod_manifest.minimal.json** — Minimal manifest that passes official-build enforcement (required fields + `source_url` + `simrard_policy`).
- **mod_manifest.full.json** — Full Modrinth-style manifest with optional fields (description, body, issues_url, wiki_url, license, game_versions, dependencies).

Manifests that omit `source_url` or use a different `simrard_policy` value (or omit it) are rejected by the official Simrard build.
