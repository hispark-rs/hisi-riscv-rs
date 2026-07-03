# Diataxis Classification Reference

Use this file when deciding where documentation belongs or when auditing drift.

## The Four Modes

### Tutorials

Purpose: help a learner make progress and gain confidence.

Good signs:
- A guided path with a beginning, middle, and end.
- The reader can follow without making many decisions.
- Success is experiential: "I ran it and saw it work."

Bad signs:
- Exhaustive option matrices.
- Long architecture rationale.
- Complete API lists.
- Many unrelated tasks.

### How-To Guides

Purpose: help a user accomplish a specific task.

Good signs:
- Clear prerequisites.
- Ordered steps.
- Commands that can be copied.
- Troubleshooting for common failures.

Bad signs:
- Teaching from first principles.
- Storing canonical facts that other pages must repeat.
- Turning into a design essay.

### Reference

Purpose: provide accurate facts for lookup.

Good signs:
- Tables, exact names, command syntax, environment variables, markers, API boundaries.
- Clear version/date/evidence when facts can change.
- Dry, complete, and easy to search.

Bad signs:
- Long "why" sections.
- Step-by-step learning narrative.
- Duplicated inventories already owned elsewhere.

### Explanation

Purpose: build understanding.

Good signs:
- Architecture, rationale, tradeoffs, policies, historical context.
- Links to reference pages for exact facts.
- Explicit boundaries between current policy and dated review history.

Bad signs:
- Owning precise current lists such as API inventories, HIL marker tables, command matrices, or build matrices.
- Repeating reference facts that will drift.

## This Repository's Canonical Patterns

- `docs/src/reference/10-stable-api.md`: current HAL stable/unstable API boundary.
- `docs/src/reference/02-examples.md`: current WS63 example inventory and marker strings.
- `docs/src/reference/07-hil-markers.md`: HIL script environment variables and script behavior; not the full example marker source.
- `docs/src/reference/06-image-format.md`: image/header/hash/signature layout facts.
- `docs/src/explanation/policies/`: policy and rationale; link to reference facts instead of duplicating long inventories.
- `docs/src/explanation/components/`: component architecture and review narrative; avoid owning current exhaustive fact tables when reference pages exist.
- `docs/review/`: dated review ledger. Preserve history, but do not use it as current truth without checking reference/code.
- `ROADMAP.md`: current plan/status, but should link out to reference for precise inventories.
- `CLAUDE.md` / `AGENTS.md`: agent operating guide. Keep summaries and links; avoid duplicating canonical lists.

## Drift Checklist

Search for these before and after substantial doc edits:

- The same table/list appears in two places.
- A page says "current", "only", "default", "stable", "verified", or "latest" without a date or source.
- A policy page contains a long API inventory.
- A review page is linked as if it were current reference.
- Script paths mention old agent-specific directories when a neutral `.agents/skills` path exists.
- HIL counts, example counts, default-members, or release behavior appear in prose outside their canonical page.
- A command output/path assumes an old target directory.

## Review Output Shape

Lead with findings. For each finding include:

- file and line;
- category: Diataxis mismatch, duplicate fact source, stale fact, missing navigation, broken verification path, or unclear audience;
- why it matters;
- a concrete fix.

If no actionable issues are found, say so and mention residual risks such as unverified links or commands not run.
