# 仓库与发布模型：把底座做成可复用的积木

这篇解释为什么本项目不是一个“所有东西绑在一起”的单仓 SDK，而是把 SVD/PAC、runtime、HAL、示例与工具拆成可以独立发布、独立复用的层。具体发版命令见[发布 crate 与父仓 release](../how-to/11-release.md)；每层内部怎么实现见[组件深入文档](components/00-index.md)。

## 核心倾向：底座应该可以被别人拿走

这个生态的目标不是把 `hisi-hal` 变成唯一正确的上层选择。更理想的形态是：

- 有人只想要 **PAC**，自己写一个完全不同风格的 HAL；
- 有人只想要 **runtime/linker/critical-section**，上面接自己的 BSP 或 C/Rust 混合工程；
- 有人接受我们的 **HAL**，但不用我们的 examples、runner 或工程模板；
- 有人只参考我们的 **HIL / QEMU / flash tooling**，但上层应用和驱动都重做。

这些路径都应该是正当的。我们做的 Rust 栈是一套被验证过的组合，但不应该把“使用 WS63/BS2X 的 Rust 生态”收窄成“必须使用我们整套选择”。越靠底层的东西越应该像公共地基：小、清晰、可发布、可替换。

所以仓库边界不是按“谁写的代码”划分，而是按**可解耦的契约边界**划分：

- PAC 的契约是寄存器类型与中断枚举；
- runtime 的契约是启动、链接脚本、入口、中断默认符号与临界区实现；
- HAL 的契约是 safe driver API、`embedded-hal` trait、stable/unstable 门控；
- examples 的契约只是“一个可运行的组合演示”，不是平台的唯一入口。

每一层都可以被保留、替换或绕过。

## 为什么是独立仓库，而不只是 workspace crate

父仓 workspace 很适合做集成：一次 clone 能看到所有组件，CI 能验证“这些版本放在一起能不能跑”，HIL 能用同一套脚本把驱动和示例上板。但 workspace 不等于发布边界。

独立仓库提供的是另一种能力：

- **独立版本节奏**：PAC 的寄存器修正、runtime 的启动修复、HAL 的 API 收窄，不必强行使用同一个版本号。
- **清晰的下游依赖**：外部用户写 `ws63-pac = "0.2"` 或 `hisi-riscv-rt = "0.4"`，不需要理解父仓 submodule 指针。
- **可替换的上层**：如果别人重写 HAL，只需要依赖 PAC/rt；我们的 HAL 不会因为和父仓绑死而挡住它。
- **可审计的发布物**：每个 crate 自己的 CI、`Cargo.lock`、tag 和 crates.io 包，能说明“这个发布物是怎么解析依赖并被检查的”。
- **故障隔离**：一个 experimental example 或父仓集成脚本坏掉，不应该阻塞一个寄存器修复版 PAC 的发布。

代价也是真的：发版顺序更麻烦，submodule pointer 要更新，`Cargo.lock` 要分清归属，父仓和子仓 CI 要避免漂移。我们接受这点复杂度，是因为它换来的是生态层面的自由度。

## 父仓是什么：集成台，而不是唯一入口

`hisi-riscv-rs` 父仓的角色更像一张工作台：

- 把各个子仓 pin 到一组已知能一起工作的 commit；
- 用根 `[patch.crates-io]` 把 registry 依赖重定向到本地 submodule，便于同时开发和避免双 PAC 实例；
- 承载文档、HIL 脚本、QEMU/真机验证、示例和父仓 firmware release；
- 记录“这一组组合”是否能 build、上板、冒烟。

它不是下游必须依赖的发布单位。应用开发者可以只用 crates.io 上的 crate 和模板工程，不 clone 父仓；另一个 HAL 作者也可以只拿 PAC/rt。父仓 release 只说明“这个集成组合被打包成了 firmware assets”，不代表所有子 crate 在同一刻一起发布。

这也是为什么父仓不能替子仓 publish crates.io：那会把独立组件重新绑回一个中心化发布动作，反而削弱了每层自己的所有权。

## 为什么发布仓用 crates.io 依赖，本地开发用 `[patch.crates-io]`

发布仓必须模拟外部用户看到的世界。外部用户不会有我们的 sibling checkout，也不会有父仓 submodule 目录；他们只会从 crates.io 解析版本。所以独立 crate 的 `Cargo.toml` 应该写 registry version dependency，CI/publish 也按 registry 解析。

本地开发又需要另一件事：在父仓里同时改 PAC、rt、HAL 时，要让它们链接到同一份本地源码。根 `[patch.crates-io]` 正好表达这个意思：

- 对外：`hisi-riscv-rt` 依赖 `ws63-pac = "0.2"`；
- 在父仓开发时：这个 registry 依赖被 patch 到 `crates/pac/ws63-pac`；
- 发布时：子仓自己的 CI 不依赖父仓 patch，按 crates.io 解析。

这不是两套事实源，而是 Cargo 的两层语义：**manifest 声明公共契约，workspace patch 声明本地集成替换**。

PAC 还有一个额外约束：全程序不能链接两份 PAC。PAC 内部的 singleton 和中断符号必须只有一份。父仓 `[patch.crates-io]` 同时解决了“用本地源码开发”和“避免双 PAC 实例”这两个问题。

## 为什么 library crate 也提交自己的 `Cargo.lock`

普通 Rust library crate 常见做法是不提交 `Cargo.lock`，因为最终应用会解析自己的完整依赖图。这个规则对“单纯被别人依赖的库”很合理，但这里的子仓还有另一个身份：它们是**独立 release unit**。

当一个 crate 自己打 tag、自己跑 CI、自己 publish，它就需要记录“这次 release 检查时解析到的依赖集合”。提交 `Cargo.lock` 的目的不是替下游锁版本；下游仍然用自己的 lockfile。它的目的有三个：

- CI 的 `--locked` 能发现 release 输入漂移；
- publish preflight 能确认 tag 指向的提交包含完整解析状态；
- 回看一个旧 tag 时，能复现当时 release 检查的依赖图。

这也是为什么 lockfile 归属跟仓库走：子仓 release 用子仓 `Cargo.lock`，父仓集成用父仓 `Cargo.lock`。父仓 lockfile 不能替代子仓 release lockfile。

## 为什么要按依赖顺序发布

独立发布意味着每层都通过 crates.io 版本和下游对话。下游的 lockfile 不能解析到一个还没发布的上游版本。

所以顺序必须从底往上：

```text
PAC/SVD → hisi-riscv-rt → hisi-hal → examples/RF/guide → 父仓 pointer
```

这个顺序不是仪式，而是依赖解析的自然结果。比如 HAL 需要消费新版 PAC，PAC 就必须先发布；父仓要 pin 到新版 HAL，HAL 的 commit 就必须先 push。父仓 pointer 永远是最后一步，因为它记录的是“已经存在、可 fetch、可复现的一组提交”。

## “官方组合”与“可替换上层”可以同时存在

项目仍然需要一个官方组合：否则文档、HIL、模板、示例、QEMU parity 都没有锚点。官方组合的意义是给用户一条可靠路径，也给维护者一个可以验证的集成面。

但官方组合不应该变成生态边界。我们要同时保留两种能力：

- **收敛**：对新用户，模板 + HAL + rt + PAC 是最短路径；stable API 必须有真机证据。
- **开放**：对高级用户，PAC/rt/tooling 可以单独拿走；上层 HAL、调度器、驱动模型、应用框架都可以重做。

这也是 `stable` / `unstable` 门控和独立 release 模型的共同点：默认路径要可靠，但实验路径不能被消灭。区别只是稳定性承诺不同。

## 这套模型会强迫我们承担的纪律

原子化不是把目录拆碎就完了。它要求维护者持续守住几个边界：

- 发布 crate 不带本地 path/git 依赖；
- 子仓 CI 和 publish 必须能在没有父仓的情况下运行；
- 父仓用 `[patch.crates-io]` 做本地集成，不把 patch 逻辑复制进子仓；
- 子仓先 commit/push/tag，父仓后更新 pointer；
- 操作步骤放在 how-to，精确事实放在 reference，设计理由放在 explanation，避免三处都维护同一张表。

如果这些纪律松掉，独立仓库会退化成“看起来很多 repo，实际上仍然只能从父仓内部工作”的伪解耦。我们现在的 release 规则就是为了防止这种退化：每个底座都应该能独立站住，也能在父仓里组合起来。
