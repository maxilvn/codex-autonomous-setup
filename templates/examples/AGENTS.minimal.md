# Minimal Codex Workflow

Use this when you want Codex to be careful, but not fully autonomous.

## How This Mode Works

Codex inspects the repo, makes targeted edits, runs checks, and reviews the diff
-- but stays local. It does not create branches, worktrees, or PRs. It commits
only when you explicitly ask.

## Rules

- Inspect the repository before making changes.
- Keep changes small and scoped to the user's request.
- Do not overwrite unrelated user work.
- Run relevant tests or checks after changes.
- Review the diff before reporting done.
- Commit only when the user asks.
- Do not create branches or worktrees.
- Do not push or open PRs unless explicitly asked.
- Report changed files and checks run.

## Completion Report

When files changed, end with:

```text
Branch: <current-branch>
Worktree: none (working in main checkout)
Commit: <commit-hash-or-"none">
PR: none
```
