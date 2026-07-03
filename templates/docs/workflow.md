# Workflow

This setup turns Codex into a predictable GitHub-first coding agent by making
the expected lifecycle explicit.

## 1. Understand The Task

Codex should first determine whether the user is asking for:

- Research
- Planning
- Implementation
- Review
- Follow-up on an existing PR

Research-only tasks should not become implementation tasks without user
approval.

## 2. Inspect Current State

Before edits, Codex should inspect:

- Current branch
- Default branch
- Dirty files
- Existing worktrees
- Project test and build commands

This prevents accidental commits of unrelated local work.

## 3. Create Isolated Work

For repo changes, Codex should create a dedicated worktree and
`codex/<short-task-name>` branch from the default branch.

This makes the task easy to review, abandon, or merge without disturbing the
main checkout.

## 4. Implement Narrowly

Codex should make the smallest coherent change that satisfies the task. It
should reuse existing patterns instead of inventing a new architecture.

## 5. Verify

Codex should run relevant checks:

- Tests
- Lint
- Type checks
- Build
- Visual verification for UI changes

The exact commands should come from the repository, not from assumptions.

## 6. Self-Review

The `post-implementation-review` skill requires Codex to inspect its diff and
remove common problems before it reports done.

This catches avoidable issues such as:

- Debug code
- Unused imports
- Over-broad abstractions
- Missing tests
- Broken imports
- Inconsistent style

## 7. Publish

When GitHub workflow is enabled, Codex should:

1. Commit the scoped changes.
2. Push the branch.
3. Open a pull request.
4. Report the branch, worktree, commit, and PR URL.

## 8. Follow Up

For PR feedback, Codex should update the existing PR branch rather than creating
a new branch.
