# GitHub PR Workflow

Use this skill when a coding task should result in a committed branch and pull
request.

## Purpose

Keep Codex's repo work isolated, reviewable, and easy to merge by using a
predictable branch, worktree, commit, push, and PR flow.

## When To Use

Use this skill for:

- Features
- Bug fixes
- Refactors
- Documentation changes
- Test additions
- Any task the user expects to land through GitHub

Do not use this skill for:

- Research-only tasks
- Quick command output
- Local-only experiments explicitly requested by the user

## Workflow

1. Inspect repository state.

   ```sh
   git status --short --branch
   git remote -v
   git branch --show-current
   ```

2. Find the default branch.

   Prefer the upstream default branch when available:

   ```sh
   git symbolic-ref refs/remotes/origin/HEAD
   ```

   If unavailable, inspect the remote with GitHub CLI:

   ```sh
   gh repo view --json defaultBranchRef
   ```

3. Update the default branch.

   ```sh
   git fetch origin
   git switch <default-branch>
   git pull --ff-only
   ```

4. Create a task worktree.

   ```sh
   git worktree add -b codex/<short-task-name> <absolute-worktree-path> origin/<default-branch>
   ```

5. Work only inside the task worktree.

   Do not hop between the main checkout and the task worktree during the same
   task.

6. Stage only task files.

   ```sh
   git status --short
   git add <task-files>
   ```

7. Commit with a concise message.

   ```sh
   git commit -m "<type>: <summary>"
   ```

8. Push the branch.

   ```sh
   git push -u origin codex/<short-task-name>
   ```

9. Open a pull request.

   ```sh
   gh pr create --fill
   ```

10. Report final state.

```text
Branch: <branch-name>
Worktree: <absolute-path-to-worktree>
Commit: <commit-hash>
PR: <url-or-"none">
```

## Existing PR Follow-Ups

When the user asks to revise an existing PR:

1. Identify the PR branch.
2. Use its existing worktree if present.
3. If missing, create or attach a worktree for that branch.
4. Commit and push back to the same branch.
5. Do not create a new branch unless the user asks.

## Guardrails

- Never include unrelated dirty work in the commit.
- Never run `git reset --hard` or `git checkout --` unless explicitly requested.
- If the default branch has local changes, create a clean worktree from
  `origin/<default-branch>` instead of modifying it.
- If GitHub CLI is missing or unauthenticated, finish local verification and
  report that PR creation could not run.
