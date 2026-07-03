# Full Codex Autonomous Workflow

Use this when Codex is allowed to behave like a GitHub-first coding agent.

## Standing Instructions

- For feature work, fixes, refactors, and docs changes, use a dedicated
  `codex/<short-task-name>` branch.
- Use a clean git worktree for the task.
- Keep unrelated dirty work out of the commit.
- Implement, verify, self-review, commit, push, and open a pull request.
- Run the post-implementation review loop before reporting complete.
- If the user says research-only, do not implement.
- If a check fails, fix the issue and rerun the check.

## Skills

Use these skill files as reusable guidance:

- `skills/github-pr-workflow/SKILL.md`
- `skills/post-implementation-review/SKILL.md`

## Completion Report

Every turn where files changed must end with:

```text
Branch: <branch-name>
Worktree: <absolute-path-to-worktree>
Commit: <commit-hash>
PR: <url-or-"none">
```

Also state which checks ran and whether changes were pushed.
