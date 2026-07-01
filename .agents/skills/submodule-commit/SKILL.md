---
name: submodule-commit
description: Commit + push changes that span ws63-rs git submodules in dependency order, then bump the parent repo's submodule pointers. Use when you've edited files inside one or more submodules (ws63-pac/hal/rt/examples/guide, chips/ws63/rf/ws63-RF, ws63-pac/ws63-svd) and need to land them correctly.
disable-model-invocation: true
---

ws63-rs is a monorepo of standalone submodules. A change inside a submodule is NOT
visible to the parent until you (1) commit inside the submodule, (2) push it, and
(3) update + commit the parent's submodule pointer. Do them in dependency order so
the parent never points at an unpushed commit.

## Submodule facts (verify each time — branches differ!)

| Submodule | Default branch | Notes |
|-----------|----------------|-------|
| ws63-pac | `main` | upstream of hal/rt; **nests ws63-svd** |
| ws63-svd | `main` | source of the PAC — **nested submodule at `crates/pac/ws63-pac/ws63-svd`** |
| hisi-riscv-rt | `master` | runtime |
| hisi-riscv-hal | `master` | drivers; depends on pac |
| ws63-examples | `master` | blinky |
| ws63-RF | `main` | blobs — parent submodule at nested path `chips/ws63/rf/ws63-RF` |
| ws63-guide | `main` | docs |

Dependency order for committing: **svd → pac → rt → hal → examples/RF/guide → parent**.

Two paths are nested, which changes the pointer-bump chain:
- **ws63-svd** is a submodule *of ws63-pac* (not of the parent). To land an svd
  change: commit+push svd → in ws63-pac bump `ws63-svd` + commit+push pac → in
  the parent bump `ws63-pac`. (Two-level.)
- **ws63-RF** is a parent submodule whose path lives inside the in-tree crate
  `ws63-rf-rs` (`chips/ws63/rf/ws63-RF`). It bumps directly in the parent like any
  other submodule — no extra level.

The in-tree `ws63-flashboot` and `ws63-rf-rs` are NOT submodules — they commit
with the parent.

## Procedure

For each submodule with changes (`git -C <sub> status -s`), in the order above:

1. **Be on its default branch.** If detached, create/checkout the branch at the
   current pinned HEAD (do NOT jump to a different commit):
   ```bash
   # if HEAD is detached but already a branch tip, just checkout; otherwise:
   git -C <sub> rev-parse --abbrev-ref HEAD          # "HEAD" means detached
   git -C <sub> branch --show-current
   # advance the local branch to the pinned HEAD only if it is an ancestor (no loss):
   git -C <sub> merge-base --is-ancestor <branch> HEAD && git -C <sub> checkout -B <branch>
   ```
2. **Commit** with a clear message; end with the Co-Authored-By trailer:
   ```bash
   git -C <sub> add -A
   git -C <sub> commit -m "<subject>" -m "Co-Authored-By: Codex Opus 4.8 <noreply@anthropic.com>"
   ```
3. **Push** to its branch:
   ```bash
   git -C <sub> push origin <branch>
   ```

Then the **parent** repo:

```bash
git add <changed-submodules> Cargo.lock <other-parent-files>   # NOT transient agent worktrees
git commit -m "chore: update submodule pointers — <what>" -m "Co-Authored-By: Codex Opus 4.8 <noreply@anthropic.com>"
git push origin main
```

## Verify

```bash
git submodule status            # each SHA must equal the submodule's pushed HEAD
git status -s                   # clean (except transient agent worktrees)
```

## Gotchas

- **Push the submodule BEFORE bumping the parent pointer** — otherwise the parent
  references a commit nobody can fetch.
- **Detached HEAD that is AHEAD of the local branch**: use `checkout -B <branch>` only
  after confirming the old branch is an ancestor (`merge-base --is-ancestor`), so you
  fast-forward and lose nothing.
- **Never add transient agent worktrees** — they are harness state, not repo content.
- **Cargo.lock** changes belong with the parent commit (it records the new pointers
  indirectly via path/patch deps).
