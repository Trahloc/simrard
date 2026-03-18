---
name: Baseline Agent Practices
description: Foundational behavioral instructions for AI agents regarding failure modes, git safety, tracking intent, and warnings. Read this for all general coding constraints.
---

# Baseline Agent Practices

These rules apply to every task in every repository. They dictate how to interact with a codebase, handle failures, manage tech debt, and operate safely in git.

**Precedence**: If the project has a `CONTRIBUTING.md`, `AGENTS.md`, or equivalent, follow that document first. This skill provides sensible defaults only when no project-specific guidance exists.

---

## 1. Orientation Before Coding

On first interaction with any repo:

1. Run `git remote -v` to classify repo topology:
   - **Upstream clone**: origin push = `UPSTREAM_DO_NOT_PUSH` → push to `fork` remote
   - **Own fork / personal project**: push to `origin` normally
2. Scan for guidance (stop at first match per category):
   - **AI rules**: `AGENTS.md`, `CLAUDE.md`, `.cursorrules`, `.agents/`
   - **Contribution guide**: `.github/CONTRIBUTING.md`, `CONTRIBUTING.md`
   - **PR template**: `.github/PULL_REQUEST_TEMPLATE.md`
3. Discover build/test/lint commands from `package.json`, `Makefile`, `Cargo.toml`, `pyproject.toml`, etc.
4. Run existing checks before anything else. If none exist, verify the project at minimum builds.

---

## 2. Fail-Fast Policy

**No silent fallbacks for systems we control.** Fail immediately on errors with structured, actionable output.

### Error Message Format

```text
ERROR: [Component] [Operation] failed
CAUSE: [Root cause or violated invariant]
CONTEXT:
  - File: [path:line]
  - Variable: [name]=[value]
  - State: [current state]
REQUIRED ACTION: [What needs to be fixed]
DIAGNOSTIC: [Additional info or command to run]
```

### Error Propagation Rules

- Enable strict modes: `set -euo pipefail` (Bash), strict `tsconfig.json` (TS), etc.
- Never suppress errors via `|| true`, `except: pass`, or empty `catch {}` without explicit handling.
- Bubble errors up with additional context.

### The One Exception: External Dependencies

Silent fallbacks are **acceptable only for external API/network calls** (e.g., CDN unreachable). But you MUST log a loud `WARNING` before falling back:

```bash
if ! fetch_external_resource; then
    echo "WARNING: External dependency unreachable. Using fallback.
CAUSE: Timeout fetching https://...
REQUIRED ACTION: Verify network connectivity." >&2
    use_local_fallback
fi
```

#### ❌ Bad — Silent Fallback
```bash
if [ -f "$CONFIG" ]; then
    source "$CONFIG"
else
    source "/default/config"  # Silent fallback!
fi
```

#### ✅ Good — Loud Failure
```bash
[ -f "$CONFIG" ] || {
    echo "ERROR: Required config file missing
CAUSE: CONFIG file not found
CONTEXT: File $CONFIG
REQUIRED ACTION: Create config or fix CONFIG variable" >&2
    exit 1
}
source "$CONFIG"
```

---

## 3. Warnings as Errors

Compiler and linter warnings indicate real problems. Treat them as build-failing errors.

- **Always configure tools to fail on warnings** (but still print the warning text so it can be actioned).
- Do not use pragmas/decorators to hide warnings without the user's explicit consent.

| ❌ Do NOT Use | ✅ Use Instead |
|---|---|
| Rust: `#[allow(unused)]` | Fix the unused symbol, or stub + `TODO` |
| TS: `// eslint-disable-next-line` | Fix the type signature |
| TS: `@ts-ignore` | Use `@ts-expect-error` with a detailed comment |
| Python: `# type: ignore` | Use `mypy --warn-error` in CI |

**Planned-but-unimplemented code**: Use `TODO` comments and remove or stub the code. Do not leave dead code or unused symbols "for later."

### Acceptable Suppression (User-Approved Only)

```rust
// User approved (2026-03-14): Kept for future v2.0 feature.
// TODO(2026-04-14): Remove once v2.0 parsing is complete.
#[allow(dead_code)]
pub fn v2_parser_mock() {}
```

---

## 4. Tech Debt Tracking (TODO Conventions)

Make tech debt searchable and actionable. Standardize all inline comment markers.

| Marker | Purpose |
|---|---|
| `TODO:` | Future work, planned features, improvements |
| `FIXME:` | Known bugs or broken code |
| `NOTE:` | Non-obvious implementation details or architectural warnings |
| `HACK:` | Temporary solutions or workarounds |

### The ISO Date Requirement

For any `HACK`, `FIXME`, or `TODO` covering a temporary solution or disabled feature, **you MUST include the ISO 8601 creation date**:

```rust
// HACK(2026-03-14): Workaround for upstream event ordering bug #1234
// FIXME(2026-03-14): Integer overflow on 32-bit systems when payload > 2GB
// TODO(2026-03-14): Re-enable caching once the redis cluster is migrated.
```

*Undated TODOs are acceptable only for wishlist items, but dating is always preferred.*

### The 30-Day Reinvestigation Rule

When you encounter a **dated marker older than 30 days**, proactively inform the user:
1. State that the marker is `N` days old.
2. Summarize what it says.
3. Recommend reinvestigating (e.g., "Check if the upstream bug is patched").
4. Let the user decide whether to fix, extend the date, or ignore it.

### Format Guidelines
- Explain **WHY**, not just **WHAT**: `// TODO(2026-03-14): Fix integer overflow on 32-bit when payload > 2GB`
- Link to trackers when applicable: `// FIXME(2026-03-14, #42): ...`

---

## 5. Git Safety

### Hard Rules (All Repo Types)

- **NEVER commit on the default branch** (`main`/`master`). Always work on a named branch.
- **NEVER force-push** unless explicitly asked.
- Before ANY commit: run `git branch --show-current` to confirm you are not on the default branch.
- If on the default branch: `git checkout -b <branch-name>` first.

### Branch Naming

| Purpose | Pattern | Example |
|---|---|---|
| Feature PR | `trahloc/feat/<name>` | `trahloc/feat/add-auth` |
| Bug fix PR | `trahloc/fix/<name>` | `trahloc/fix/null-check` |
| Maintenance PR | `trahloc/chore/<name>` | `trahloc/chore/update-deps` |
| Local throwaway | `local/<name>` | `local/experiment` |

- `local/*` branches are **never pushed**.
- `trahloc/*` branches are pushable to `fork` or `origin` depending on repo type.

### Commits

- Use [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `test:`
- Add scope when the repo has multiple packages: `feat(api): add endpoint`
- Explain **why** in the commit message body, not just what changed.
- One logical change per commit.

### PR Workflow

```bash
# Start from latest default
git fetch origin && git checkout -b trahloc/feat/<name> origin/main

# Sync before PR
git fetch origin && git rebase origin/main

# Push (upstream clone)
git push fork trahloc/feat/<name> -u

# Push (fork / personal)
git push origin trahloc/feat/<name> -u
```

### Upstream Clone Setup (Fresh Clone of Someone Else's Repo)

```bash
git config --local remote.origin.pushurl UPSTREAM_DO_NOT_PUSH
git remote add fork git@github.com:trahloc/<repo-name>.git
git checkout -b local/run origin/main
```

---

## 6. Local File Management

- Local tool configs (editor rules, `.envrc`, etc.) belong in `.git/info/exclude`, not `.gitignore`, so they stay local without polluting upstream.
- **Exception**: Some repos commit these files intentionally — always check before adding them to exclude.
