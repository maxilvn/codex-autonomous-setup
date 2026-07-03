# Minimal Codex Workflow

Use this when you want Codex to be careful, but not fully autonomous.

## Rules

- Inspect the repository before making changes.
- Keep changes small and scoped to the user's request.
- Do not overwrite unrelated user work.
- Run relevant tests or checks after changes.
- Review the diff before reporting done.
- Commit only when the user asks.
- Report changed files and checks run.

## Completion Report

When files changed, end with:

```text
Branch: <branch-name>
Worktree: <absolute-path>
Commit: <commit-hash-or-"none">
PR: none
```
