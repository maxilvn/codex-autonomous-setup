# Codex GitHub Harness

An opinionated open-source workflow harness that makes Codex behave like a
disciplined GitHub coding agent.

## Quick Start

```sh
npx codex-github-harness init
```

Or install globally:

```sh
npm install -g codex-github-harness
codex-github-harness init
```

The installer will ask you:

- Full autonomous or minimal workflow
- Skills global (`~/.codex/skills/`) or local (`./skills/`)
- Branch prefix (default: `codex/`)
- Worktree directory pattern

It writes or appends `AGENTS.md` in your repo, installs skills, copies docs and
examples, and shows next steps.

### Dry Run

```sh
npx codex-github-harness init --dry-run
```

### Target a Specific Repo

```sh
npx codex-github-harness init /path/to/your/repo
```

## What This Is

Codex is most useful when the operating rules are explicit. This harness gives
Codex a consistent workflow for repository work:

- Use clean task branches and git worktrees.
- Keep unrelated dirty work out of feature changes.
- Verify changes before reporting done.
- Run a post-implementation self-review loop.
- Commit, push, and open a pull request when the task is intended for GitHub.
- Report the final branch, worktree, commit, and PR state clearly.

The goal is not to make Codex "magical." The goal is to make its behavior
predictable, reviewable, and close to how a careful human engineer would work.

## Included Files

- `AGENTS.md` - the full autonomous workflow instruction set.
- `templates/` - all files the installer copies into your repo.
- `skills/github-pr-workflow/SKILL.md` - reusable GitHub branch, worktree,
  commit, push, and PR workflow.
- `skills/post-implementation-review/SKILL.md` - reusable post-change review and
  cleanup loop.
- `examples/AGENTS.minimal.md` - smaller version for teams that want fewer
  rules.
- `examples/AGENTS.full.md` - full version with skill references and stricter
  reporting.
- `docs/workflow.md` - how the workflow runs from task intake to PR.
- `docs/customization.md` - what to change before using this in your own
  environment.
- `docs/faq.md` - common questions and tradeoffs.
- `CONTRIBUTING.md` - contribution guidelines for improving the setup.

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
