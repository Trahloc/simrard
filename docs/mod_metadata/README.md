# Mod metadata (Modrinth-style + Simrard enforcement)

Simrard mods use a **Modrinth-style** metadata schema so tooling and authors can reuse familiar fields. The **official game build** enforces two extra requirements before loading any mod: a public source URL and acceptance of the Simrard Open Mod policy.

## Schema overview

Each mod provides a manifest (e.g. `mod.json` or `manifest.json`) at the mod root. Field names and semantics align with [Modrinth's project/version model](https://docs.modrinth.com/api/) where applicable.

### Required fields (all builds)

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Unique mod identifier (slug). Regex: `^[\w!@$().+,\-']{3,64}$`. |
| `title` | string | Display name. |
| `version` | string | Semantic version (e.g. `1.2.0`). |
| `authors` | array of strings | Creator(s) of the mod. |

### Required for official build (Simrard enforcement)

These are **required** for the mod to load in the **official** Simrard binary. Builds compiled from source may omit this check.

| Field | Type | Description |
|-------|------|-------------|
| `source_url` | string | **Required.** Public URL where the mod's source is available (e.g. Git repo). Must be non-empty and a valid URL. |
| `simrard_policy` | string | **Required.** Must be exactly `Simrard-Open-Mod-1.0`. Indicates the mod accepts the [Simrard Open Mod Pledge](OPEN_MOD_PLEDGE.md). |

### Optional (Modrinth-style)

| Field | Type | Description |
|-------|------|-------------|
| `description` | string | Short description. |
| `body` | string | Long-form description (e.g. README). |
| `issues_url` | string | Where to report bugs. |
| `wiki_url` | string | Wiki or docs. |
| `license` | object | License of the mod's code/assets. |
| `license.id` | string | SPDX identifier (e.g. `MIT`, `Apache-2.0`). |
| `license.name` | string | Full license name. |
| `license.url` | string | URL to the license text. |
| `dependencies` | array | Other mods or game version requirements (see examples). |
| `game_versions` | array of strings | Supported Simrard game versions (e.g. `["0.1.0"]`). |

The official build does **not** require `license`, `issues_url`, or `wiki_url`; they are optional and recommended for clarity.

## Enforcement rules (official build)

1. **`source_url`** must be set and non-empty; the loader may validate URL format (e.g. `http`/`https`). No public repo ⇒ mod is rejected.
2. **`simrard_policy`** must be exactly `Simrard-Open-Mod-1.0`. Any other value or missing field ⇒ mod is rejected.
3. Builds compiled from source may disable or change these checks; only the official Simrard build is required to enforce them.

Mods that do not meet these requirements are **not loaded** by the official build and are considered unapproved. No legal claim is made over the mod's copyright; this is a runtime and policy gate.

## Example layout

```
my_mod/
├── mod.json          # manifest (required)
├── transforms/       # transform files (per 02_mod_and_data_architecture)
│   └── ...
└── ...
```

See [examples/](examples/) for minimal and full manifest examples.

## References

- [Modrinth API – Get project](https://docs.modrinth.com/api/operations/getproject/)
- [Simrard Open Mod Pledge](OPEN_MOD_PLEDGE.md)
- [Mod & data architecture](../02_mod_and_data_architecture.md)
