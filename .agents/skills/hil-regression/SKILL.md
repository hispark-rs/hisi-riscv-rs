---
name: hil-regression
description: Diagnose a HIL CI failure on real WS63/BS2X silicon — compare the failed CI run against the last passing run, identify whether the regression is caused by a code change, toolchain update, or rig issue, and output a triage report. Use when a HIL CI job fails on the main branch or a PR.
disable-model-invocation: true
---

# HIL Regression Diagnosis Skill

Diagnoses HIL CI failures on real silicon. Run when a `hil.yml` CI job fails on
a commit that should be safe (e.g. a docs-only change fails HIL → rig issue;
a DMA change fails HIL → likely code regression).

**User-invoked** (`/hil-regression`). It reads CI logs, does NOT flash hardware.

---

## Usage

```bash
# Diagnose the latest failed HIL CI run (autodetects run ID):
bash .agents/skills/hil-regression/regression.sh

# Diagnose a specific run:
bash .agents/skills/hil-regression/regression.sh --run 28502888903

# Show pass/fail history for the last 30 days:
bash .agents/skills/hil-regression/regression.sh --history
```

---

## Diagnosis methodology

### Phase 1 — Gather data

```bash
# 1. Find the latest failed HIL CI run:
gh run list --workflow hil.yml --limit 5 --json databaseId,conclusion,headBranch,displayTitle

# 2. Fetch the failed job's logs:
gh run view <run-id> --log --job <job-name> > /tmp/hil-fail.log

# 3. Find the last passing run on the same branch:
gh run list --workflow hil.yml --branch main --limit 10 --json databaseId,conclusion
gh run view <passing-run-id> --log > /tmp/hil-pass.log

# 4. Check what code changed between the two runs:
git log --oneline <passing-sha>..<failing-sha>
git diff --stat <passing-sha>..<failing-sha>
```

### Phase 2 — Classify failure

| Symptom | Likely cause | Action |
|---------|-------------|--------|
| No UART output at all | Boot hang: wrong loaderboot, flash address, or power | Check `ADDRESS` and `LOADERBOOT` in the CI config. Compare with hil-smoke --preflight. |
| Garbled UART | Baud mismatch: clock-tree change affects UART divider | Check `TIMER_CLOCK_HZ` / UART clock source in the diff. |
| Specific test FAILED | Code regression in the exercised driver | `git diff <passing-sha>..<failing-sha> -- crates/hisi-riscv-hal/src/<driver>.rs` |
| All tests FAIL (incl. ones not touched) | **Rig issue**: board disconnected, probe-rs failed, power cycled | Check the CI runner logs for hardware errors, USB resets, timeout on `probe-rs run`. |
| Intermittent (passes locally) | Board contact / wiring / timing edge | Re-run HIL CI (`gh run rerun <run-id>`). If 2/3 passes → wiring flaky, not code. |

### Phase 3 — Produce report

```markdown
# HIL Regression Report

## Commit
<failing-sha> by <author> on <date>
<commit-message>

## CI Run
<url-to-run>
<workflow> / <job> — <conclusion>

## Diff vs last passing
<files changed, summary>

## Diagnosis
**<RIG / CODE / TOOLCHAIN / INTERMITTENT>**

<detailed evidence>

## Next step
- If RIG: <what to check on the board / CI runner>
- If CODE: <which file/line is suspect, what to fix>
- If TOOLCHAIN: <pin toolchain version, revert to known-good>
- If INTERMITTENT: <re-run CI, add retry logic if flaky>
```
