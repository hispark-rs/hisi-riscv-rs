# Safe / Unsafe 政策：把信任边界写清楚

这是本项目 HAL（0.6.0+）的**第三号约定**：`unsafe` 可以存在，但必须被模块边界、类型不变式、SAFETY 注释和验证流程约束住。对外的 safe API 不能让调用者触发 UB；否则这个 API 不能稳定暴露。

本篇讲**为什么**要有这条政策、**怎么判断**一段 unsafe 是否被安全封装、以及它**如何接到稳定 / 不稳定 API 门控**。完整调研见 `docs/review/safe-unsafe-formal-verification-research-2026-07.md`，当前基线见 `docs/review/unsafe-audit-2026-07-01.md`。

## 问题：HAL 不可能没有 unsafe

裸机 HAL 的核心工作就是直接碰硬件：读写 MMIO 寄存器、操作 cache CSR、把 DMA 缓冲区地址交给外设、声明中断向量和启动符号。这些事编译器无法证明安全，所以必然需要 `unsafe`。

真正的问题不是“有没有 unsafe”，而是：**safe 调用者能不能通过公共 API 把 unsafe 的前提打破。**

```rust
// UNSOUND：safe 调用者可以传越界 idx，触发 UB
pub fn read_unchecked(arr: &[u32], idx: usize) -> u32 {
    unsafe { *arr.get_unchecked(idx) }
}
```

这段代码里 `unsafe` 很短，但封装是坏的。调用者没有写 `unsafe`，却能触发 UB。对 HAL 来说，对应的坏形态是：`pub fn` 接收任意寄存器偏移、任意 DMA 长度、任意裸地址、任意通道号，然后未经检查直接进入 `unsafe {}`。

## 标准：soundness，不是“看起来能跑”

本仓采用 Rust unsafe code guidelines 的核心标准：

> 一个 unsafe 抽象是 sound 的，当且仅当 safe 代码无法通过它的公共 API 触发 UB。

这意味着：

- HIL 跑过不等于 sound。HIL 证明“这个硬件路径在这块板上工作”，不能穷尽所有 safe 输入。
- SAFETY 注释不是装饰。它必须写清 unsafe 依赖的前提，以及 safe API 如何强制这些前提成立。
- 封装边界是模块。Rustonomicon 的经验规则是：限制 unsafe 影响范围最可靠的方法是 **module boundary + privacy**。
- 形式化证明不是默认门禁。Miri/Kani/Verus 可以验证 host-testable 逻辑和关键状态机，但 MMIO、DMA、cache、时序仍要靠 stub、人工审阅和 HIL 组合。

## HAL 里的 unsafe 主要来自哪里

Rust Reference 只允许少数几类操作需要 unsafe；本仓主要命中这些场景：

| unsafe 操作 | HAL 场景 |
|---|---|
| 裸指针解引用 | `&*Peripheral::ptr()` 取得 MMIO register block |
| 调 unsafe fn | PAC `bits()` 写寄存器、cache maintenance、外设 singleton `steal()` |
| inline asm | cache CSR、低功耗 / 中断相关指令 |
| static / 全局状态 | DMA 通道 claim、waker/中断状态 |
| unsafe trait impl | 手写 `Send` / `Sync` 承诺 |
| unsafe 属性 / extern | 启动符号、中断向量、FFI 边界 |

其中最多的是 MMIO。MMIO 不是普通内存访问：地址必须是这颗芯片真实存在的寄存器，字段值必须是硬件接受的组合，访问顺序还可能有前置时钟、复位、FIFO、DMA handshake 等要求。

## SAFETY 注释分级

每个 `unsafe {}`、`unsafe fn`、`unsafe impl` 都要有对应的 `// SAFETY:` 或 `# Safety` 说明。审计时按四级判定：

| 等级 | 标准 | 处置 |
|---|---|---|
| **A** | 写清地址有效性、值有效性、前置条件、后置状态 | 可接受 |
| **B** | 写清地址 / 不变式，前置条件由类型或私有 helper 隐含 | 可接受，后续可补强 |
| **C** | 只说“这是有效寄存器”之类，缺少前提和不变式 | 需要改进；不作为毕业加分项 |
| **D** | 没有 SAFETY 注释 | 必须修；相关 API 不能凭这个状态毕业为 stable |

推荐写法：

```rust
// SAFETY: `spi_dr` is the SSI v151 data register at offset 0x60 from the
// SPI0 base. `tx` is a valid 8-bit frame because `DataBits` only exposes the
// byte data path. The SPI controller was configured and enabled by `new_spi0`;
// this write only pushes one FIFO entry and does not touch control bits.
unsafe { r.spi_dr().write(|w| w.bits(tx as u32)) }
```

不合格写法：

```rust
// SAFETY: register access.
unsafe { r.spi_dr().write(|w| w.bits(tx)) }
```

这句只说明“我知道它是寄存器”，没有说明地址、值、状态机和 safe API 前提。

## 封装规则

新增或修改 unsafe 代码时，按这些规则判断是否能对外暴露成 safe API：

- **字段默认私有。** 只要一个字段参与 unsafe 不变式，就不要让下游 crate 直接写它。
- **safe 参数必须先解析。** 来自用户的长度、索引、地址、通道号、频率、位宽，必须先变成校验过的类型或返回 `Result` / `Option`，不能直接喂给 unsafe。
- **unsafe helper 优先私有。** 如果一个 helper 的调用者必须满足硬件前提，把它做成私有函数，由同模块 safe API 建立前提；不要把负担甩给下游。
- **Drop 要先停硬件再释放资源。** DMA / 外设传输 guard 的 `Drop` 必须 cancel-then-quiesce，避免缓冲区释放后硬件仍在写。
- **unsafe impl 要写理由。** `unsafe impl Send` / `Sync` 必须说明为什么跨上下文移动不会破坏独占、别名或中断不变式。
- **不要用 HIL 掩盖 UB 风险。** 真机通过只能证明一个路径工作；若 safe 输入空间里仍有能触发 UB 的值，封装仍然 unsound。

## 和稳定 / 不稳定门控的关系

[稳定 / 不稳定 API 门控](02-stable-unstable.md) 的规则是“默认只暴露 HIL 真机验证过的 API”。Safe/Unsafe 政策在它下面再加一条负面约束：

> 有 Tier D SAFETY 注释缺口或确认 safe→unsafe unsound forwarding 的 API，不能毕业为 stable。

也就是说：

| 状态 | 对 API 稳定性的影响 |
|---|---|
| HIL 未覆盖 | 保持 `unstable` |
| HIL 已覆盖，但 unsafe 封装有确认 soundness 问题 | 先修 soundness，不能 stable |
| HIL 已覆盖，只有 Tier C 注释 | 可继续人工判断；毕业前应补强 |
| HIL 已覆盖，unsafe 封装 A/B 且无确认 soundness 问题 | 满足 safe/unsafe 侧毕业条件 |

这条规则避免“上板跑过一次”被误解成“所有 safe 输入都安全”。HIL 是硅片行为证据，unsafe 审计是 Rust 抽象边界证据，两者缺一不可。

## 验证分层

本仓不追求一次性给整个 HAL 做 RustBelt 级别的全 soundness 证明。实际路线是分层、可复现、逐步收敛：

| 层 | 工具 / 方法 | 覆盖什么 |
|---|---|---|
| 编译期 | 所有权 token、lifetime、typestate、sealed trait | 外设独占、非法配置不可表达 |
| 注释审计 | `clippy::undocumented_unsafe_blocks` + 人工审阅 | 每个 unsafe 的前提是否写清 |
| 静态候选 | safe→unsafe forwarding 扫描 | 找出需要人工判定的公共 API |
| host 测试 | unit / property tests | 纯逻辑边界、newtype 构造、状态机编码 |
| Miri | host-testable 路径 | 动态 UB / aliasing 候选 |
| Kani / Verus | 关键纯逻辑函数 harness | 穷尽检查长度、计数、溢出、状态转换 |
| HIL | 真实 WS63 硅片 | 寄存器序列、时钟、DMA、IRQ、cache 的实际行为 |

AI 辅助验证只能作为候选生成器：可以帮助写审计队列、harness 草稿或规约草稿，但不能替代工具输出和人工审阅。被信任的是 clippy/Miri/Kani/Verus/Lean kernel/HIL 的可复现结果，不是 LLM 的自然语言结论。

## 落地流程

涉及 unsafe 的 PR 或 release 前，按这个顺序做：

1. **列清单**：`bash .agents/skills/safe-unsafe-verify/verify.sh --audit-only`，记录 unsafe 分布和 safe→unsafe 候选。
2. **补 SAFETY**：先补 Tier D，再补高风险 Tier C。DMA、cache、interrupt、peripheral singleton 优先。
3. **人工判定候选**：对每个 safe→unsafe forwarding，确认 safe API 是否已经校验了长度、范围、状态和所有权。
4. **跑 clippy baseline**：`bash .agents/skills/safe-unsafe-verify/verify.sh`，记录 `undocumented_unsafe_blocks` 收敛情况。
5. **补 host 测试 / harness**：优先验证纯逻辑边界，如 DMA beats 上限、timeout tick 计算、typed config 构造器。
6. **上板验证**：只有涉及硬件行为的结论，必须用 HIL 或明确标成未验证。
7. **接稳定门控**：有 HIL 且无确认 soundness 缺口，才考虑从 `unstable` 毕业。

当前基线（2026-07-01）：`crates/hisi-riscv-hal/src` 有 486 处 `unsafe` 命中，`clippy::undocumented_unsafe_blocks` 捕获 390 个 warning；Miri 和 Kani 还没有形成门禁。这是**整改起点**，不是已完成状态。

## 参考实现方向

优先把这些模块作为 P0 收敛对象：

- `dma.rs`：缓冲区所有权、cache maintenance、channel claim、Drop quiesce，是 unsafe 风险最高的模块。
- `spi.rs` / `uart.rs`：公共操作 API 多，容易出现 safe 参数进入 MMIO / DMA 路径。
- `gpio.rs` / `peripherals.rs`：singleton 和 `steal()` 是资源独占模型的根。
- `cache.rs` / `interrupt.rs`：inline asm、CSR、全局中断状态需要更强 SAFETY 注释。

不要把这个政策理解成“所有 unsafe 都要删掉”。正确目标是：**unsafe 短、局部、可审计；safe API 的不变式由类型、privacy 和验证流程共同守住。**
