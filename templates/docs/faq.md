# FAQ

## Is this a Devin replacement?

No. This is an operating manual for Codex. It makes Codex behave more like a
disciplined GitHub coding agent by giving it clear rules for branches,
worktrees, verification, review, and reporting.

## Why use worktrees?

Worktrees keep task changes isolated from the main checkout. They reduce the
chance that Codex accidentally commits unrelated dirty files and make it easier
to abandon or revise a task branch.

## Why make post-review a skill?

The post-review loop is useful across repositories, so it works well as a
reusable skill. Keeping it separate also makes `AGENTS.md` easier to read while
preserving detailed review instructions.

## Should Codex always open a PR?

No. Use PR creation only when the repository is meant to be managed through
GitHub review. For local-only work, use the minimal example or tell Codex not to
push.

## What if checks fail?

Codex should fix the failure and rerun the relevant check. If the same issue
persists after repeated attempts, it should report the exact blocker instead of
claiming completion.

## What if the repository has no tests?

Codex should run whatever validation exists, such as a build, type check,
linter, formatter check, or documentation link check. If no meaningful checks
exist, it should say so plainly.

## Can teams edit these rules?

Yes. This repository is intended as a starting point. Teams should adapt branch
naming, worktree location, verification commands, and PR policy to match their
own workflow.
