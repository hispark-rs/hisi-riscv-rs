# `cargo-hisi` Developer Workflow CLI Plan

**Status:** Deferred until the WS63 plain-`cargo build` and single-dependency
`hisi-rf` path are proven. This is an independent tooling track and does not
block the current WPA3/connectivity milestone.

## Summary

建立独立 `cargo-hisi` host tool/release unit，为人类开发者和自动化智能体提供一致的
HiSilicon Rust 工程入口：发现环境、构建、生成镜像、烧录、监控和测试。Cargo package 与
可执行文件均命名为 `cargo-hisi`，用户命令为：

```bash
cargo hisi doctor
cargo hisi build
cargo hisi image
cargo hisi flash --monitor
cargo hisi test
```

建议仓库为 `hispark-rs/cargo-hisi`。`cargo-hisi` 是薄编排层，不是新的构建系统或格式
事实源。普通 `cargo build` 必须始终可用；不安装 CLI 不能改变 crate 的可构建性、链接结果
或固件语义。

## Scope And Non-Goals

`cargo-hisi` 负责：

- 通过 Cargo metadata 发现 workspace、binary、chip/profile、target 和产物；
- 检查官方 Rust toolchain、target、`rust-src`、外部 transport 和设备权限；
- 委托 Cargo 构建，委托 `hisi-fwpkg` 生成镜像；
- 委托 `hisiflash` 或通用 probe-rs/J-Link transport 烧录和复位；
- 统一 monitor、marker、timeout、artifact manifest、错误和机器输出；
- 为 host/QEMU/manual HIL 提供可发现、可复现的测试入口。

明确非目标：

- 不复制 HiSilicon header/hash/body/partition 语义；这些只属于 `hisi-fwpkg`。
- 不解析或修补 RF archive、ROM symbol、relocation；这些属于 `ws63-radio-sys`/
  `hisi-rf-link`，且普通用户路径最终必须只使用标准 relocation artifact。
- 不重写 Cargo dependency resolution、feature selection、rustc/linker 或 test harness。
- 不把 `hisiflash`、`hisi-fwpkg`、probe-rs 或未来 `hisi-rtos-cli` 合并为一个 release unit。
- 第一阶段不做 BSP/board manager、IDE daemon、GUI、云服务、软件包 registry 或常驻 HIL
  runner。

## Naming And Release Boundary

- GitHub repository：`hispark-rs/cargo-hisi`。
- Cargo package/binary：`cargo-hisi`，入口命令为 `cargo hisi`。
- 第一阶段单 crate，内部按模块分层；有第二个真实消费者后才提取共享 `hisi-cli-core`。
- `cargo-hisi` 独立版本、`Cargo.lock`、CI、release notes 和跨平台 binary artifacts，不与
  HAL/RT/RF 版本强行同号。
- 发布 compatibility manifest，记录支持的 project metadata schema、Rust nightly、
  `hisi-fwpkg`、`hisiflash`、probe-rs capability 和芯片 profile 范围。
- 未来若建立 standalone `hisi` umbrella，它只委托同一 command handlers；不得复制
  `cargo-hisi`、`hisi-fwpkg`、`hisiflash` 或 `hisi-rtos-cli` 实现。

## Architecture And Dependency Direction

```text
Developer / Agent
  -> cargo-hisi
       -> cargo metadata + cargo process
       -> hisi-fwpkg library       # image truth source
       -> hisiflash library         # serial transport/monitor
       -> probe-rs executable       # generic SWD/JTAG transport, initial integration
       -> JLinkExe                  # optional external hardware-reset/debug transport
```

第一阶段直接依赖 `hisi-fwpkg` 与 `hisiflash` library，获得结构化错误而不是解析人类输出。
probe-rs 初期采用外部 executable + 明确版本/capability detection，减少大依赖和 fork 耦合；
只有 attach/run 所需的通用 library capability 稳定且确有收益后，才评估直接 library 集成。

CLI 不通过 shell script 编排，不依赖 Bash、PowerShell、Python、`sed`、`awk`、GNU coreutils
或个人绝对路径。所有 path 使用 Rust `Path`/`OsStr`，所有进程参数独立传递，不拼 shell
command string。

## Project And Configuration Contract

构建事实源仍是 Cargo manifest、features 和 `rust-toolchain.toml`。例如：

```toml
hisi-rf = {
    version = "0.2",
    features = ["chip-ws63", "profile-wifi-wpa3-smoltcp"]
}
```

`cargo-hisi` 不再维护一份 chip/profile 配置。可提交的非构建提示使用版本化 Cargo
metadata：

```toml
[package.metadata.hisi]
schema = 1
board = "ws63-evb"
default-binary = "wifi-demo"
```

本机 probe/serial/board alias 放入用户配置目录，不进入项目仓库，也不覆盖 Cargo 的 chip/
profile 选择。CLI 必须显示每项配置的来源与优先级；command line 显式值优先，环境变量只用于
secret 或 CI 注入，项目 metadata 不记录设备序列号和凭据。

每次成功 build/image/run 都产生版本化 artifact manifest，至少包含：

- workspace/package/binary/target/profile；
- rustc/Cargo/tool versions 与 lockfile hash；
- ELF/image/FlashPlan path、size 和 SHA-256；
- chip、RF/runtime/memory profile revision；
- transport capability/version；
- source revision 与 dirty-state 标记。

## Command Contract

### `cargo hisi doctor`

检查项目 metadata、官方 toolchain、target/build-std、linker、`hisi-fwpkg` library compatibility、
probe-rs/J-Link/hisiflash capability、USB/serial 权限和匹配设备。默认只读，不安装、不修改系统
安全策略；失败必须提供可执行下一步。

### `cargo hisi metadata`

输出解析后的 chip/profile、binaries、memory/resource report、构建 target、可用 transport、
HIL marker 和 capability status。它读取 owner 输出的 machine facts，不把文档表格当输入。

### `cargo hisi build`

委托 Cargo，保留标准 feature/profile/locked/offline 语义；增加前置 contract check、产物定位、
resource summary 和 artifact manifest。不得在构建后秘密 patch 一个 plain Cargo 无法生成的
ELF。

### `cargo hisi image`

把选中 ELF 交给 `hisi-fwpkg` library 的 `FlashPlan`/image API。CLI 不推导 app base、header、
hash、body range、gap 或 erase range。

### `cargo hisi flash` / `monitor`

按显式 `--transport`、`--device`、`--chip` 和 artifact manifest 选择 hisiflash、probe-rs 或
J-Link。`monitor` 只读串口；`attach` 默认不得 reset/flash；只有 `flash`/`run` 可以改变目标，
并必须在执行前输出 plan。

### `cargo hisi run`

组合 `build -> image -> flash -> reset -> monitor/expect`，但每一步仍使用上述公共 contract，
可单独重跑并在失败后从 artifact manifest 恢复。取消时必须清理临时进程和设备锁，并明确
报告目标最后已知状态。

### `cargo hisi test`

发现 host、QEMU 和 manual HIL tests。普通命令不安装/注册 self-hosted runner；硬件测试必须
显式选择设备、获取互斥锁并保存结构化 evidence。测试结果不能把 transport failure 混算成
firmware failure。

## Agent And Automation Contract

从 C0 起，每个命令必须支持：

- `--json`：版本化 schema，stdout 只输出 machine result；人类 diagnostics 进入 stderr。
- `--no-prompt`：禁止菜单和隐式设备选择；缺参数时 fail closed。
- `--timeout`：覆盖 discovery、build、transport、monitor 和 marker wait。
- `--no-color`：稳定日志与 CI artifact。
- `--plan`/`--dry-run`：在不构建、不烧录或不复位时输出将执行的动作。
- 稳定退出码类别：usage/config、environment、build、image、transport、timeout/cancel、
  target-test failure；具体数值在 C0 冻结。

机器错误至少包含 `schema_version`、`command`、`stage`、`class`、稳定 code、原始 bounded
diagnostics、artifact/target state 和安全的 `next_actions`。next action 必须声明是否只读、
是否修改目标、是否需要人工授权，不能只输出自然语言建议。

设备访问必须使用跨进程互斥锁，lock identity 包含 transport + serial/probe identity，支持
owner PID、超时和 stale-lock recovery。两个 CLI/agent 不能同时烧录、reset 或 monitor 同一
设备。JSON/non-interactive 模式禁止自动选择“第一个设备”。

SSID/passphrase、registry token、private key 和 key material 只允许通过临时环境/secret
provider/受控 stdin 注入；不得进入 argv 回显、artifact manifest、JSON、日志、crash dump、
shell history 或 repository。所有输出类型必须有 secret-redaction tests。

## Milestones

### C0 -- Contract Freeze

- [ ] 建仓、license、independent lock/release policy 和 supported-host matrix。
- [ ] 冻结命令树、global flags、JSON schema、exit-code categories、artifact manifest 和
  configuration precedence。
- [ ] 写清 plain Cargo invariant、owner/delegation table、无隐藏下载和无常驻 runner边界。
- [ ] 提供 fake project/device/transport，先写 golden CLI/JSON/error tests。

### C1 -- Metadata And Doctor

- [ ] 使用 `cargo_metadata` 发现 workspace/package/binary/target/features，校验 exactly-one
  chip 和命名 profile。
- [ ] 实现只读 toolchain/target/build-std/tool/USB/serial doctor 与 actionable next action。
- [ ] 读取版本化 `package.metadata.hisi` 和用户设备 alias，输出每项配置来源。
- [ ] macOS arm64、Linux x64、Windows x64 CI 通过，无 shell/Python dependency。

### C2 -- Build And Image

- [ ] 委托 Cargo build/check，保留 `--locked`、`--offline`、feature 和 message-format。
- [ ] 定位唯一产物并生成 artifact manifest；多 binary 时 non-interactive 必须显式选择。
- [ ] 调用 `hisi-fwpkg` library 生成 plan/image，并与 standalone CLI 对同一输入逐字节一致。
- [ ] WS63 Wi-Fi consumer 证明不需要 guarded-link、sys metadata、GCC/Python 或绝对路径。

### C3 -- Flash And Monitor

- [ ] 集成 hisiflash library，支持 FWPKG/app image、显式 LoaderBoot 和降速恢复策略。
- [ ] 集成 probe-rs executable capability detection，保持零 HiSilicon image-format magic。
- [ ] 增加 J-Link optional reset/attach adapter，但不把 J-Link 变成默认依赖。
- [ ] 完成设备锁、plan、cancel/timeout、stale process cleanup、UART marker 与 binary-safe log。

### C4 -- Run And Test

- [ ] `run` 使用可恢复 stage state machine；失败报告区分 build/image/download/reset/firmware。
- [ ] `test` 发现 host/QEMU/manual HIL，复用 repository executable contracts，不复制命令。
- [ ] 在 WS63 上完成 UART hello 与 Wi-Fi init/scan/connect/ping manual HIL；不部署本机 runner。
- [ ] 保存 artifact/evidence manifest，能够证明同一 ELF/image 在不同 transport 下的 parity。

### C5 -- Agent And Safety Closure

- [ ] 所有命令完成 JSON/no-prompt/timeout/no-color/plan contract 和 golden schema tests。
- [ ] 覆盖多设备、设备忙、权限不足、中途取消、断线、超时、损坏 artifact、版本不兼容和
  stale lock；错误必须给出 bounded next action。
- [ ] secret redaction、路径转义、命令注入、恶意 project metadata 和不可信 artifact manifest
  测试通过。
- [ ] 提供 `cargo hisi capabilities --json` 或等价 metadata view，状态只来自发布 manifest、
  CI/HIL evidence，不从文档文字猜测。

### C6 -- Release And Ecosystem Integration

- [ ] 发布 crates.io `cargo-hisi` 与 macOS arm64、Linux x64、Windows x64 GitHub assets。
- [ ] template/happy-path docs 使用同一 executable snippets；`--help`、JSON schema 和命令
  reference 从 CLI source 生成或由 drift check 校验。
- [ ] 保留 standalone Cargo/hisi-fwpkg/hisiflash/probe-rs 操作指南，证明 CLI 不是锁定层。
- [ ] 建立 compatibility/CVE/dependency radar 和 release rollback instructions。

## Test Matrix

- Host unit：metadata merge、configuration precedence、artifact selection、error mapping、locks、
  redaction、path handling、timeout/cancel state machine。
- Golden contract：`--help`、JSON schema、exit codes、plan、artifact manifest 和 next actions。
- Cross-OS consumer：macOS arm64、Linux x64、Windows x64；路径包含空格、非 ASCII、长路径，
  read-only Cargo registry、offline cache 和并发 workspace build。
- Delegation parity：Cargo direct vs `cargo hisi build` ELF；`hisi-fwpkg` direct vs image bytes；
  hisiflash/probe-rs direct vs transport result。
- Failure injection：缺工具、版本漂移、USB 拔出、串口占用、download verify failure、reset failure、
  marker timeout、Ctrl-C 和 agent deadline。
- Hardware：手动 WS63 UART smoke 与 connectivity smoke；transport failure 和 firmware marker
  failure 分开计数。普通 PR 不要求用户日常电脑成为 self-hosted runner。

## Acceptance

以下路径在新生成、无父仓 patch/submodule 的项目中成立：

```bash
cargo install cargo-hisi
cargo hisi doctor --no-prompt
cargo build --release
cargo hisi image --json
cargo hisi run --device <explicit-device> --timeout 120 --json
```

- 不安装 `cargo-hisi` 时，plain Cargo 产生相同 ELF。
- 用户不直接依赖或调用 `ws63-radio-sys`、`hisi-rf-link`、`hisi-rf-rtos-driver`。
- 三个 host OS 的 build/image 路径不依赖 shell、Python、GCC/binutils 或个人路径。
- JSON/non-interactive 路径无菜单、无隐式设备、无 secret 泄漏，且失败后目标状态可解释。
- `cargo-hisi` 产物与底层 owner tools 逐字节/逐阶段一致；CLI 没有第二份格式或能力事实源。

## Assumptions

- `hisi-rf` single-dependency facade 与标准 relocation/plain Cargo build 先于 C2 完成。
- 第一阶段服务 Cargo/Rust firmware project，不承担任意 C SDK 工程管理。
- `hisi-fwpkg`、`hisiflash`、probe-rs 与 `hisi-rtos-cli` 保持独立 release unit。
- 当前连接性 WIP=1 不因 CLI 建仓而改变；本计划启动需要独立排期和 maintainer capacity。
