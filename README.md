# Codex GitHub Harness

![Codex GitHub Harness preview](assets/social-preview.jpg)

Codex GitHub Harness installs an `AGENTS.md` workflow and reusable Codex skills
into a repository, so Codex handles coding tasks through clean branches,
worktrees, verification, self-review, commits, pushes, and pull requests.

## What This Is

Codex is most useful when the operating rules are explicit. This project gives
Codex those rules.

Use it when you want Codex to behave less like a chat assistant editing files
ad hoc, and more like a GitHub-first engineering agent whose work is isolated,
checked, and reviewable.

It is not a hosted service, a replacement for GitHub, or a bot that merges code
for you. It is a small installer that copies workflow instructions, examples,
and reusable skills into a repo.

## What It Changes

Without this harness, a Codex coding session can be ambiguous: should Codex edit
the current checkout, create a branch, run tests, commit, push, or stop after a
local patch?

With this harness, the expected behavior is written down:

- Use clean task branches and git worktrees.
- Keep unrelated dirty work out of feature changes.
- Verify changes before reporting done.
- Run a post-implementation self-review loop.
- Commit, push, and open a pull request when the task is intended for GitHub.
- Report the final branch, worktree, commit, and PR state clearly.

The goal is not to make Codex "magical." The goal is to make its behavior
predictable, reviewable, and close to how a careful human engineer would work.

## Before And After

Before:

```text
You ask Codex to fix something.
Codex may edit the current checkout directly unless you give more instructions.
```

After:

```text
You ask Codex to fix something.
Codex creates a task branch and worktree, makes the fix there, runs checks,
self-reviews the diff, commits, pushes, opens a PR, and reports the final state.
```

## Example Workflow

Example task:

```text
Fix the failing login test.
```

With the full workflow, Codex should:

1. Inspect the current branch, default branch, dirty files, worktrees, and
   available test commands.
2. Create a task branch such as `codex/fix-login-test` in a dedicated worktree.
3. Make the smallest targeted code change that fixes the issue.
4. Run the relevant tests, linter, type checker, or build.
5. Review its own diff and remove debug code, dead code, unused imports, and
   unnecessary abstractions.
6. Commit the scoped changes, push the branch, and open a pull request.
7. Report the branch, worktree, commit, PR URL, and checks that ran.

## Quick Start

```sh
npx codex-github-harness init
```

Or install globally:

```sh
npm install -g codex-github-harness
codex-github-harness init
```

The installer walks you through four questions and sets up everything.

### 1. Workflow Mode

- **Full autonomous** -- Codex creates branches, uses worktrees, runs
  verification, self-reviews the diff, pushes, and opens PRs. The complete
  disciplined-engineer workflow.
- **Minimal** -- Codex inspects the repo, makes targeted edits, runs checks, and
  reviews the diff, but stays local. No branches, no worktrees, no PRs. Commits
  only when you explicitly ask.

### 2. Skill Scope

- **Global (`~/.codex/skills/`)** -- Skills are available in every Codex session
  on this machine, regardless of which repo you are in.
- **Local (`./skills/`)** -- Skills are installed into the current repo and
  version-controlled with the project. Best for team setups.

### 3. Branch Prefix

- **`codex/` (default)** -- Press Enter to accept. Codex creates branches like
  `codex/fix-auth-bug`.
- **Custom** -- Enter a prefix without trailing slash (e.g. `ai`, `bot`, your
  team name). The `/` is added automatically.

### 4. Worktree Directory

- **`./worktrees/<task>` (default, inside repo)** -- Worktrees live inside the
  repo directory, easy to find and clean up.
- **`../worktrees/<task>` (outside repo)** -- Worktrees live outside the repo,
  keeping your main checkout clean.
- **Custom** -- Enter any path relative to the repo root.

### Dry Run

```sh
npx codex-github-harness init --dry-run
```

### Target a Specific Repo

```sh
npx codex-github-harness init /path/to/your/repo
```

## What Gets Installed

The installer writes or updates a small set of plain-text files:

- `AGENTS.md` -- the workflow rules Codex reads when it works in the target
  repo.
- `skills/` -- reusable Codex skills for PR workflow and post-change review.
- `docs/` -- explanations of the workflow, customization options, and FAQ.
- `examples/` -- minimal and full `AGENTS.md` variants for different autonomy
  levels.

## Included Skills

The installer copies two reusable Codex skills:

- **pr-merge-cleanup** -- Branch, worktree, commit, push, and PR flow for GitHub
  tasks. Keeps repo work isolated, reviewable, and easy to merge.
- **post-implementation-review** -- Self-review loop: diff check, cleanup,
  re-verify before reporting done. Catches bugs, dead code, and style drift.

## Repository Contents

- `AGENTS.md` -- the full autonomous workflow instruction set used by this
  repository.
- `templates/` -- all files the installer copies into your repo.
- `skills/pr-merge-cleanup/SKILL.md` -- reusable PR merge, branch deletion,
  commit, push, and PR workflow.
- `skills/post-implementation-review/SKILL.md` -- reusable post-change review
  and cleanup loop.
- `examples/AGENTS.minimal.md` -- smaller version for teams that want fewer
  rules.
- `examples/AGENTS.full.md` -- full version with skill references and stricter
  reporting.
- `docs/workflow.md` -- how the workflow runs from task intake to PR.
- `docs/customization.md` -- what to change before using this in your own
  environment.
- `docs/faq.md` -- common questions and tradeoffs.
- `CONTRIBUTING.md` -- contribution guidelines for improving the setup.

## Recommended Setup

Use the full `AGENTS.md` when:

- Codex is allowed to create branches and PRs.
- Your repo has meaningful tests, linting, or type checks.
- You want Codex to keep working through verification failures.
- You want a consistent completion report after changes.

Use the minimal example when:

- You mainly want local edits.
- You do not want Codex opening PRs automatically.
- You are still evaluating how much autonomy to allow.

## Requirements

- Node.js 18 or higher
- Git
- GitHub CLI (`gh`) for PR creation
- A GitHub account authenticated through `gh auth login`
- A repository with clear test, lint, and type-check commands

Codex can still use the workflow without GitHub CLI, but it should report that
PR creation could not run.

## Safety Model

This setup intentionally asks Codex to be autonomous, but not careless:

- It uses worktrees to isolate changes.
- It avoids mixing unrelated dirty files into commits.
- It requires diff review before completion.
- It requires verification before reporting done.
- It reports exact git state at the end of code-changing turns.

You should still review every pull request before merging it.

## License

MIT. See `LICENSE`.
