# 如何发布 crate 与父仓 release

本篇给维护者一套可照做的发布流程：发布独立 crate 子仓到 crates.io，或发布父仓的 firmware GitHub Release。为什么这样拆仓和独立发版见[仓库与发布模型](../explanation/08-repository-release-model.md)；HIL 命令见[如何运行 HIL 测试](07-run-hil-tests.md)；stable API 边界以[Stable API 清单与门控状态](../reference/10-stable-api.md)为准。

## 0. 先选发布目标

不要在父仓一次性替所有东西发版。每个目标由自己的仓库和 workflow 拥有：

| 目标 | 在哪里 tag | 产物 |
| --- | --- | --- |
| `ws63-pac` | `crates/pac/ws63-pac` 子仓 | crates.io |
| `bs2x-pac` | `crates/pac/bs2x-pac` 子仓 | crates.io |
| `hisi-riscv-rt` | `crates/hisi-riscv-rt` 子仓 | crates.io |
| `hisi-riscv-hal` | `crates/hisi-riscv-hal` 子仓 | crates.io |
| 父仓 `hisi-riscv-rs` | 仓库根 | GitHub Release firmware assets；不发布子 crate |

如果一次改动跨多个子仓，按依赖顺序发：

```text
PAC/SVD → hisi-riscv-rt → hisi-riscv-hal → examples/RF/guide → 父仓 pointer
```

依赖方的 `Cargo.lock` 会解析到已发布的 crates.io 版本，所以先发布上游，等 crates.io index 可见后再发布下游。

## 1. 发布前硬规则

这些规则不满足就不要打 tag：

- 在**目标仓库**的默认发布分支上 release，不在 detached HEAD 上打 tag。
- 独立发布的 Rust crate 必须提交自己的 `Cargo.lock`，即使它是 library crate。
- 发布仓的 `Cargo.toml` 使用 crates.io 版本依赖；本地 checkout 只由父仓根 `Cargo.toml` 的 `[patch.crates-io]` 接管。不要把 path/git patch 放进要发布的 crate manifest。
- `Cargo.lock` 必须已跟踪、干净，并能通过 `--locked`：

```bash
cargo generate-lockfile --locked
git ls-files --error-unmatch Cargo.lock
git diff --exit-code -- Cargo.lock
cargo package --locked --no-verify
```

- 改 public API 时同步更新 rustdoc/手册。HAL stable/unstable 变化必须同步更新 [Stable API 清单](../reference/10-stable-api.md)，并有对应真机 HIL 证据。
- 子仓 commit 必须先 push，再更新父仓 submodule pointer。父仓不能指向别人 fetch 不到的提交。

## 2. 发布一个 crate 子仓

下面以 `hisi-riscv-hal` 为例；其他 crate 把路径换成对应子仓。

```bash
cd crates/hisi-riscv-hal
git status --short --branch
```

如果是 detached HEAD，先切回该仓的发布分支。不要直接跳到远端新 HEAD；确认当前改动在你要发布的提交上：

```bash
git switch master        # hisi-riscv-hal / hisi-riscv-rt
# 或:
git switch main          # ws63-pac / bs2x-pac
```

更新版本与 changelog：

```bash
$EDITOR Cargo.toml       # package.version
$EDITOR CHANGELOG.md
cargo generate-lockfile
```

做 release preflight：

```bash
cargo generate-lockfile --locked
git ls-files --error-unmatch Cargo.lock
git diff --exit-code -- Cargo.lock
cargo package --locked --no-verify
```

再跑该仓自己的 CI 等价命令。精确矩阵以子仓 `.github/workflows/ci.yml` 为准；本地至少跑 workflow 里的 locked check/doc/clippy。常用形态：

```bash
# PAC crate
cargo check --locked --all-features
cargo check --locked --all-features --release
cargo clippy --locked --all-features
cargo doc --locked --no-deps --document-private-items

# hisi-riscv-rt
cargo +hisi-riscv check --locked --target riscv32imfc-unknown-none-elf
cargo +hisi-riscv clippy --locked --target riscv32imfc-unknown-none-elf -- -D warnings
cargo +hisi-riscv doc --locked --no-deps --document-private-items --target riscv32imfc-unknown-none-elf

# hisi-riscv-hal：按 .github/workflows/ci.yml 的 feature matrix 跑
cargo check --locked --no-default-features --features chip-ws63,rt,async,embassy
cargo check --locked --no-default-features --features chip-bs21,rt,unstable
```

HAL release 如果改变 stable API、驱动行为、unsafe 封装或 HIL 证据，继续跑对应专项检查：

```bash
# HAL driver-level embedded-test；详见 how-to/07-run-hil-tests.md
PROBE_YAML=/path/HiSilicon_WS63.yaml \
CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=../../hil/embedded-test-runner.sh \
cargo test --no-default-features --features chip-ws63,rt \
    --target riscv32imfc-unknown-none-elf \
    --test hil
```

提交并推送子仓：

```bash
git add Cargo.toml Cargo.lock CHANGELOG.md .github/workflows <changed-files>
git commit -m "release: vX.Y.Z"
branch=$(git branch --show-current)
git push origin "$branch"
```

打 tag 并观察发布 workflow：

```bash
git tag -a vX.Y.Z -m "vX.Y.Z"
git push origin vX.Y.Z

# 可选：用 repo skill 代看 workflow
bash /path/to/hisi-riscv-rs/.agents/skills/release-train/train.sh vX.Y.Z
```

crates.io 没有 GitHub release asset；`publish.yml` 成功就是发布信号。若下游马上要依赖这个版本，等 crates.io index 能解析到它后再继续。

## 3. 更新父仓 submodule pointer

子仓 release 成功并 push 后，回到父仓根目录：

```bash
cd /path/to/hisi-riscv-rs
git submodule status --recursive
git status --short
```

让父仓记录新的子仓 commit；如果父仓 `Cargo.lock` 因路径成员版本变化而更新，也一起提交：

```bash
cargo generate-lockfile
git add crates/hisi-riscv-hal Cargo.lock
git commit -m "chore: bump hisi-riscv-hal to vX.Y.Z"
git push origin main
```

如果同一轮发布了多个子仓，先把所有上游子仓都发布并 push，再一次性提交父仓 pointer。

## 4. 发布父仓 firmware release

父仓 tag 只创建 firmware GitHub Release，不发布 crates.io crate。发布前确认：

```bash
git submodule status --recursive
git status --short
cargo build --release
cargo fmt --all -- --check
```

如果 release 包含 HAL stable API 或真机行为变化，按[如何运行 HIL 测试](07-run-hil-tests.md)跑对应 HIL；如果只更新文档，也至少跑：

```bash
mdbook build docs
python3 .agents/skills/diataxis-docs/scripts/audit_docs.py docs --links
python3 .agents/skills/diataxis-docs/scripts/audit_docs.py docs --current-claims
python3 .agents/skills/embedded-test-hil/scripts/check_hil_smoke_markers.py
python3 .agents/skills/embedded-test-hil/scripts/hil_inventory.py --strict
```

`--current-claims` 会列出需要人工复核的“当前 / 默认 / stable”等表述；它是漂移线索扫描，输出不等于必然错误。

然后 tag 父仓：

```bash
git tag -a vX.Y.Z -m "vX.Y.Z"
git push origin vX.Y.Z
```

父仓 `.github/workflows/release.yml` 会构建 default-members，产出 firmware release assets。它不会替子仓 publish crates.io。

## 5. 常见失败

- **`--locked` 失败**：`Cargo.lock` 缺失或过期。先正常 `cargo generate-lockfile`，提交 lockfile，再重跑 `--locked`。
- **`Cargo.lock is not tracked`**：lockfile 还是 untracked；这是 release blocker，`git add Cargo.lock` 后再提交。
- **`cargo publish` 抱怨 path/git dependency**：发布 manifest 里混进本地开发依赖。把发布依赖改回 crates.io 版本依赖；本地替换放父仓 `[patch.crates-io]`。
- **下游解析不到刚发布的上游 crate**：crates.io index 还没传播，等一会儿再跑下游 `cargo generate-lockfile`。
- **父仓指向不可 fetch 的 submodule commit**：你先更新了父仓 pointer，但忘了 push 子仓。先 push 子仓，再修正父仓 pointer commit。
- **tag 打错仓库**：crate release 必须在 crate 子仓 tag；父仓 tag 只做 firmware release。
