# Customization

Before using this setup in another repository, adjust the rules that are
environment-specific.

## Branch Names

Default task branches use:

```text
codex/<short-task-name>
```

Change this if your organization uses another convention.

## Worktree Location

The included instructions require a dedicated worktree for feature work. Choose
a predictable location, for example:

```text
../<repo-name>-worktrees/<task-name>
```

For monorepos or personal setups, you may prefer a central directory.

## GitHub Permissions

If Codex should open pull requests, make sure:

- `gh` is installed.
- `gh auth status` succeeds.
- The authenticated account can push branches.
- The repository allows PRs from pushed branches.

If you do not want Codex to open PRs automatically, use
`examples/AGENTS.minimal.md`.

## Verification Commands

Replace generic check names with your repository's real commands.

Examples:

```sh
npm test
npm run lint
npm run typecheck
cargo test
swift test
pytest
```

Codex should inspect the repository and run only commands that exist or are
clearly documented.

## Autonomy Level

You can tune how far Codex should go:

- Local-only edits
- Commit but do not push
- Push but do not open PR
- Full commit, push, and PR

The full setup is best for repositories where all changes are expected to go
through review.

## Team Policy

Consider adding explicit rules for:

- Protected files
- Generated files
- Release branches
- Required test commands
- Security review expectations
- Commit message format
