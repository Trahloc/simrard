# Agent Framework Overview

This directory contains the configuration and instruction sets for the Antigravity AI agent. Unlike editor-local rules (e.g., Cursor's `.mdc` files), this framework is designed for autonomous agents that plan, research, and execute tasks across the entire codebase.

## Structure

- **`skills/`**: Domain-specific instruction sets. Used for teaching the agent complex, multi-step behaviors or architectural patterns.
  - Each skill should be in its own subdirectory (e.g., `skills/rust-development/`).
  - Must contain a `SKILL.md` with YAML frontmatter.
- **`workflows/`**: Deterministic step-by-step guides for specific tasks (e.g., `workflows/deploy-staging.md`).
  - Defined as Markdown files with YAML frontmatter.
  - Supports annotations like `// turbo` for safe auto-execution.

## How to Interact (for LLMs/Agents)

1.  **Read this README**: Always start here to understand the agent-first philosophy.
2.  **Explore Skills**: Check `skills/*/SKILL.md` for domain expertise relevant to your current task.
3.  **Follow Workflows**: If a task matches an existing workflow in `workflows/*.md`, follow the steps exactly.
4.  **Update**: As the project evolves, update these instructions to capture new knowledge or patterns.

---

*Note: Antigravity agents are proactive. When you see these directories, use the `list_dir` and `view_file` tools to ingest the instructions before starting work.*
