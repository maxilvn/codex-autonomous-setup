# Codex Autonomous Workflow

Use these instructions when acting as a coding agent in this repository.

## Operating Principles

- Work from the actual repository state. Inspect files, commands, and git state
  before making claims.
- Prefer small, targeted changes that match the existing codebase.
- Keep implementation, tests, and verification in the same task branch.
- Do not mix unrelated dirty work into commits.
- Continue through implementation, verification, cleanup, commit, push, and PR
  creation unless the user explicitly asks for local-only work or research-only
  work.

## Task Intake

Before changing files:

1. Confirm the repository state with `git status`.
2. Identify the default branch and current branch.
3. Check for unrelated local changes.
4. Derive the likely test, lint, and type-check commands from the repo.
5. If the task is research-only, do not implement until the user explicitly
   switches to implementation.

## GitHub-First Workflow

For feature work, fixes, refactors, documentation changes, and other repo
changes:

1. Start from an up-to-date default branch.
2. Create one dedicated git worktree for the session.
3. Create a task branch named `codex/<short-task-name>`.
4. Do all edits, tests, commits, pushes, and PR updates from that worktree.
5. Stage only files that belong to the task.
6. Push the branch.
7. Open a pull request.
8. Report the PR URL and final git state.

If continuing an existing PR, use the existing branch and worktree when
available.

For detailed reusable guidance, see `skills/github-pr-workflow/SKILL.md`.

## Verification

After code changes, run the checks that match the repo:

- Tests
- Linter
- Type checker
- Build command when relevant
- Visual verification for UI-impacting changes

If a check fails, fix the failure and rerun the relevant check. If a command
cannot run, report exactly why.

## Post-Implementation Review

After implementing a feature or fix, run a self-review loop before reporting
complete:

1. Review the full diff.
2. Check for logic bugs, broken imports, missing tests, dead code, debug code,
   lint/type issues, incomplete error handling, and inconsistent style.
3. Remove unnecessary abstractions, unused code, placeholder stubs, and comments
   that merely restate the code.
4. Keep the diff focused on the requested task.
5. Rerun final checks after cleanup.

The reusable skill version lives at
`skills/post-implementation-review/SKILL.md`.

## Completion Report

Every turn where files changed must end with:

```text
Branch: <branch-name>
Worktree: <absolute-path-to-worktree>
Commit: <commit-hash>
PR: <url-or-"none">
```

Also include:

- One-line summary of what changed.
- Checks that ran.
- Checks that could not run, with the reason.
- Whether changes are pushed or local-only.

## Autonomy Boundaries

- Do not run destructive git commands unless explicitly requested.
- Do not revert user changes unless explicitly requested.
- Do not publish secrets or local credentials.
- Do not create broad abstractions unless they clearly reduce real complexity.
- Do not claim completion until the requested result is implemented and
  verified.
