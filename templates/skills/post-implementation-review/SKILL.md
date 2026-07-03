# Post-Implementation Review

Use this skill after implementing a feature or fix and before reporting
completion.

## Purpose

Make Codex pause after implementation, inspect its own diff, remove avoidable
mess, and verify the result before telling the user the work is done.

## When To Use

Use this skill after:

- Feature work
- Bug fixes
- Refactors
- Test changes
- User-facing behavior changes

Skip this skill for:

- Research-only turns
- Simple command output
- Pure planning responses
- Tiny text-only edits where a full loop would add no value

## Review Loop

Run up to three review iterations.

Each iteration:

1. Inspect the diff.

   ```sh
   git diff --stat
   git diff
   ```

2. Check for correctness issues.

   Look for:

   - Logic bugs
   - Broken imports
   - Missing tests for changed behavior
   - Incomplete error handling
   - Unhandled edge cases introduced by the change
   - UI states that would overlap, overflow, or fail on small screens

3. Check for cleanup issues.

   Remove:

   - Unused imports
   - Dead code
   - Placeholder stubs
   - Debug logging
   - Comments that simply restate the code
   - Unnecessary abstractions
   - Unrequested configuration or flexibility
   - Formatting drift unrelated to the task

4. Check for consistency.

   Confirm the change follows:

   - Existing naming patterns
   - Existing file organization
   - Existing error handling conventions
   - Existing test style
   - Existing UI or documentation tone

5. Fix any issue found.

6. Rerun the relevant checks.

   At minimum, consider:

   ```sh
   npm test
   npm run lint
   npm run typecheck
   npm run build
   ```

   Use the actual commands for the repository. Do not invent package-manager
   commands when the repo uses another tool.

## Exit Criteria

The review loop is complete when:

- The diff is scoped to the requested task.
- No obvious cleanup issues remain.
- Relevant checks pass.
- Any checks that cannot run are clearly explained.
- The branch is ready to commit and push.

If the same issue persists after three iterations, stop and report it as a
blocker with the exact failing command or unresolved problem.

## Reporting

In the final response, include:

- What changed
- Which checks ran
- Which checks could not run
- Branch, worktree, commit, and PR state when code changed
