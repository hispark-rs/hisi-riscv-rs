---
name: diataxis-docs
description: Review, write, restructure, and maintain Markdown/mdBook/repository documentation using the Diataxis framework. Use when asked to audit documentation organization, create or edit docs, choose tutorial/how-to/reference/explanation placement, prevent documentation drift, enforce a single source of truth, update SUMMARY/navigation, or verify docs after changes.
---

# Diataxis Docs

Use this skill to keep repository documentation useful, current, and organized by Diataxis: tutorials teach, how-to guides solve tasks, reference states facts, and explanation gives context.

## First Moves

1. Identify the task mode: **review**, **write**, **modify**, or **restructure**.
2. Read the local docs map before judging placement: `docs/src/SUMMARY.md`, the relevant `00-index.md`, and the target page(s).
3. Read [references/diataxis.md](references/diataxis.md) when classifying pages, moving content between sections, or reviewing organization.
4. For mechanical drift checks, run:

```bash
python3 .agents/skills/diataxis-docs/scripts/audit_docs.py docs
```

Add `--links` for local Markdown link targets and `--current-claims` when specifically hunting stale "current/default/stable" statements. The script is a scanner, not an oracle. Treat its output as leads to inspect.

## Diataxis Placement

Use this quick classifier before writing or moving content:

| User need | Put it in | Shape |
|---|---|---|
| "Teach me the path" | `tutorials/` | A learning journey with a reliable end state |
| "Help me do X" | `how-to/` | Steps, prerequisites, commands, troubleshooting |
| "Tell me exact facts" | `reference/` | Complete, terse, versioned facts and tables |
| "Explain why/how it works" | `explanation/` | Background, tradeoffs, architecture, policies |

Do not mix modes just because one page is convenient. If a page needs another mode, link to that page instead of copying the content.

## Source Of Truth Rules

- Keep current facts in reference pages, code, or explicitly current planning files.
- Keep policies in explanation pages; policies may link to reference facts but should not duplicate long fact inventories.
- Keep reviews as dated ledgers. They may preserve old facts, but must not be treated as current truth without checking newer reference/docs/code.
- When removing a duplicate fact, replace it with a link to the canonical page and a short sentence about the boundary.
- Before editing a fact, find every duplicate with `rg` and update or de-duplicate the whole set in the same change.

## Review Workflow

1. Build a small document map: affected pages, their Diataxis category, and likely canonical fact sources.
2. Check for drift risks: duplicated tables, copied status claims, old dates, old tool paths, stale command output, and "current" statements in review ledgers.
3. Check page fit:
   - tutorials should not become exhaustive reference pages;
   - how-to pages should not carry architecture essays;
   - reference pages should not explain motivation at length;
   - explanation pages should not own precise inventories or command matrices.
4. Report actionable findings first, with file and line references. Prefer concrete fixes over broad style advice.
5. If editing is requested, apply the fixes and run validation.

## Writing Workflow

1. State the page's job in one sentence before writing.
2. Pick the destination by Diataxis category. If uncertain, split: facts to reference, rationale to explanation, procedure to how-to.
3. Source facts from code, scripts, CI, existing canonical reference pages, or user-provided evidence. Do not invent current status.
4. Add or update `SUMMARY.md` and local `00-index.md` entries when adding or moving pages.
5. Use links instead of repeating canonical tables or lists.
6. Keep the page's title and opening paragraph aligned with its category.

## Modification Workflow

1. Read the page plus linked canonical sources.
2. Edit narrowly, but fix nearby contradictions introduced by the same fact change.
3. Preserve dated review history unless the user asks to rewrite it; add current links rather than erasing useful historical context.
4. For moved content, leave a short pointer where readers would expect to find it.
5. Run relevant checks:
   - `mdbook build docs` for mdBook changes;
   - `python3 .agents/skills/diataxis-docs/scripts/audit_docs.py docs` for drift leads;
   - project-specific build/test commands when docs describe runnable behavior.

## Forward Testing

For large documentation reorganizations, use independent agents only when helpful. Pass raw pages or a generic task such as "use `$diataxis-docs` to review these docs for organization and drift"; do not pass your expected findings.
