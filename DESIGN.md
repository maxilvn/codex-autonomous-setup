# GTM Agent Design Notes

The app uses Vercel's Geist design reference vendored at
`docs/vercel-design.md` as the UI baseline.

## MVP Direction

- Minimal local developer tool, not a marketing page.
- White canvas, black primary actions, gray borders, compact panels.
- Blue is reserved for focus states and links.
- No decorative gradients, glass, heavy shadows, or placeholder controls.

## First Screen

The first screen is onboarding, not a landing page:

- Codex detection status.
- One URL input.
- One analysis action.
- After creation, show workspace path, Codex run status, thread id, log path,
  and previews of the generated markdown files.

## Backend Source of Truth

The UI only reflects local files:

- `.gtm-agent/config.json`
- `.gtm-agent/runs/*.json`
- `.gtm-agent/runs/*.jsonl`
- `.gtm-agent/events.jsonl`
- `product-information.md`
- `marketing-strategy.md`
- `competitor-analysis.md`
- `brand-voice.md`
