# Branch protection (main)

`main` is protected via GitHub. Applied with:

```bash
gh api repos/Trahloc/simrard/branches/main/protection -X PUT --input .github/branch-protection-main.json
```

**Settings (SillyTavern-style, single-dev):**

- **Require a pull request** before merging (0 required approvals — single dev).
- **Enforce for admins** — no direct push to `main`; use a branch and merge via PR.
- **No force pushes** to `main`.
- **Do not allow branch deletion** for `main`.
- No required status checks (add later if CI is added).
- No required linear history.

To change: edit `branch-protection-main.json` and re-run the `gh api` command above.
