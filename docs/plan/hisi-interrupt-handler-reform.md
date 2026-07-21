# 中断处理整改规划

**版本**: 1.0
**日期**: 2026-07-21
**范围**: hisi-riscv-rt + hisi-hal + hisi-rtos
**决策**: esp-hal 路线（`set_handler` + `bind_interrupts!` 语法糖）

---

## 一、背景与现状

### 1.1 当前架构

WS63 的中断处理跨越三个 crate：

```
┌─────────────────────────────────────────────────────┐
│  hisi-riscv-rt (.S assembly)                        │
│    trap_entry / nmi_vector                          │
│    mie_interruptX_handler / local_interrupt_handler │
│    hisi_push_task_context / _pop (272B unified)     │
│    csrw mscratch, sp (四栈切换)                     │
│    __hisi_irq_epilogue → __hisi_resume_trap         │
│    weak symbols: mie0..5_interrupt_handler          │
│                  local_isr_dispatch                 │
│                  __hisi_irq_epilogue_default        │
│                  __rt_irq_dispatch                  │
├─────────────────────────────────────────────────────┤
│  hisi-hal (Rust, interrupt:: module)                │
│    中断控制器操作 (enable/disable, 优先级, 阈值)     │
│    IRQ 26-31: mie CSR                              │
│    IRQ >=32: LOCIEN/LOCIPRI/LOCIPCLR custom CSR    │
│    无 handler 注册 API                              │
├─────────────────────────────────────────────────────┤
│  hisi-rtos (Rust, context.rs + runtime.rs)          │
│    TaskContext (272B, repr(C))                      │
│    __hisi_irq_epilogue(frame) → 调度下一个任务       │
│    cooperative_context_switch_fallback              │
│    interrupt_enter() / interrupt_exit()             │
└─────────────────────────────────────────────────────┘
```

### 1.2 用户写 ISR 的当前方式

```rust
#[unsafe(no_mangle)]
extern "C" fn TIMER_INT0() {
    TimerAlarm0::clear_interrupt();
    hisi_rtos::interrupt_enter();
    // 业务逻辑
    hisi_rtos::interrupt_exit();
}
```

问题：依赖弱符号覆盖（函数名拼错不报错）、无类型检查（参数类型错误静默忽略）、无编译期验证（多 handler 重复定义同符号只有链接时冲突）。

### 1.3 为什么不走 riscv-rt 的 #[interrupt] 路线

`hisi-rtos` 的 `__hisi_irq_epilogue` 要求完整 272B unified frame（32 GPR + 32 FPR + mstatus/mepc/tp/fcsr），以支持 ISR 返回时切换到**不同于被打断任务的另一个任务**。riscv-rt 的 `#[interrupt]` 宏的核心价值是"选择性保存寄存器"——只保存 ISR 实际用到的。这与 RTOS 的完整帧语义冲突，该路线在 WS63 生态里无性能优势。

此外，维护一个定制 proc macro crate 的成本高于在 `hisi-hal` 里加 API 层。

---

## 二、目标架构

### 2.1 职责分界

```
┌─────────────────────────────────────────────────┐
│ hisi-riscv-rt (不变)                            │
│   汇编入口、上下文保存、栈切换、IRQ epilogue     │
│   weak symbol 仍然存在                          │
├─────────────────────────────────────────────────┤
│ hisi-hal (扩展)                                 │
│   new: set_handler() —— 函数指针注册            │
│   new: bind_interrupts! —— 声明宏               │
│   现有: enable/disable/init/clear_pending 不变  │
├─────────────────────────────────────────────────┤
│ hisi-rtos (不变)                                │
│   TaskContext / __hisi_irq_epilogue 不变        │
│   interrupt_enter/exit 不变                     │
├─────────────────────────────────────────────────┤
│ ws63-pac (不变)                                 │
│   ExternalInterrupt 枚举 + device.x 弱符号      │
└─────────────────────────────────────────────────┘
```

### 2.2 用户 API 目标

```rust
// 方式一：set_handler —— 动态注册
interrupt::set_handler(Interrupt::TIMER_INT0, || {
    TimerAlarm0::clear_interrupt();
    hisi_rtos::interrupt_enter();
    // 业务逻辑
    hisi_rtos::interrupt_exit();
});

// 方式二：bind_interrupts! —— 语法糖，生成 #[no_mangle] extern "C" fn
bind_interrupts! {
    TIMER_INT0 => || { /* ... */ },
    SOFT_INT0  => || { /* ... */ },
    GPIO_INT0  => || { /* ... */ },
}

// 方式三：继续用 #[no_mangle] extern "C" fn (向后兼容，不弃用)
```

### 2.3 向后兼容

已有的 `#[no_mangle] extern "C" fn TIMER_INT0()` 继续工作。`set_handler` 是新增能力，不破坏任何现有代码路径。`timer_irq` / `gpio_irq` QEMU 示例的独立手写汇编 mode 保持不变（作为绕过框架的对照基线）。

---

## 三、分步实施计划

### 阶段一（P0）：`set_handler` API

**位置**: `crates/hisi-hal/src/interrupt.rs`

**改动**:

```rust
use core::sync::atomic::{AtomicUsize, Ordering};

const HANDLER_COUNT: usize = 92;

static DEFERRED_STORE_QUEUE: [AtomicUsize; HANDLER_COUNT] = [/* init */];

/// 注册一个中断 handler。handler 在 `hisi-riscv-rt` 的 MIE/local interrupt
/// 入口处被调用，已拥有四栈隔离和 full 272B frame。
///
/// handler 必须是 `fn()` — 不使用任何参数，通过闭包捕获或静态变量与外部通信。
pub fn set_handler(irq: Interrupt, handler: extern "C" fn()) {
    DEFERRED_STORE_QUEUE[irq as usize].store(handler as usize, Ordering::Release);
}

/// 覆盖 startup.S 的 weak symbol，读取 handler 表并调用。
/// 此函数在第 2.2 层由他的 `bind_interrupts!` 宏替换
#[unsafe(no_mangle)]
unsafe extern "C" fn mie0_interrupt_handler() {
    let ptr = HANDLERS[26].load(Ordering::Acquire) as *const ();
    if !ptr.is_null() {
        // SAFETY: ptr 由 set_handler 写入，确保指向合法的 fn()
        unsafe { core::mem::transmute::<_, extern "C" fn()>(ptr)() };
    }
}
// 同理覆盖 mie1..mie5 和 local_isr_dispatch
```

**设计要点**:

- 使用 `AtomicUsize` 序列避免 `static mut` 带来的 unsafe 和 UB 风险
- `Extern "C" fn()` 类型确保 ABI 匹配
- handler 返回后，汇编框架继续执行 `csrr a0, mscratch → __hisi_irq_epilogue → __hisi_resume_trap`
- 不需要 `interrupt::free()` 包围 register——`AtomicUsize::store` 是单指令原子写

**验收**:

- `timer_irq` 和 `gpio_irq` 测试用例可通过 `set_handler` 替代手写 `#[no_mangle]` 运行
- 空 handler（未注册的 IRQ 触发）不 crash

### 阶段二（P1）：`bind_interrupts!` 声明宏

**位置**: `crates/hisi-hal/src/interrupt.rs`（或新增 `macros.rs`）

**改动**:

```rust
#[macro_export]
macro_rules! bind_interrupts {
    ($($irq:ident => $handler:expr),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            unsafe extern "C" fn $irq() {
                $handler();
            }
        )*
    };
}
```

或者支持两种形式（`FnOnce()` + `extern "C" fn`）：

```rust
#[macro_export]
macro_rules! bind_interrupts {
    // 闭包形式
    ($($irq:ident => || $body:block),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            unsafe extern "C" fn $irq() {
                $body
            }
        )*
    };
    // extern "C" fn 形式
    ($($irq:ident => $handler:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            unsafe extern "C" fn $irq() {
                $handler();
            }
        )*
    };
}
```

**设计要点**:

- 编译期为每个 handler 生成唯一的 extern "C" fn，避免符号冲突
- 闭包环境能在 handler body 中引用外围的静态变量
- 宏展开为 `#[no_mangle] extern "C" fn`，完全兼容 startup.S 的 weak symbol 覆盖

**验收**:

- `bind_interrupts!` 生成的代码与等价的手写 `#[no_mangle] extern "C" fn` 产生的二进制完全一致
- 编译期检测重复符号（linker error on symbol conflict）

### 阶段三（P2）：embassy 兼容

**无需在本 crate 实现。** embassy 的 `bind_interrupts!` 宏在其自身的 proc macro 中完成。当用户的 `Interrupt` 枚举类型实现了 `embassy_executor::Interrupt` trait 后即可直接使用 embassy 自己的宏。

验证点：确保 `ws63_pac::interrupt::ExternalInterrupt` 或 `hisi_riscv_rt::interrupt::ExternalInterrupt` 枚举中的变体名称在 embassy 的 `Interrupt` trait 中可被识别。

### 阶段四（P3）：废弃 timer_irq / gpio_irq 的手写汇编 mode（可选）

这两个 QEMU 测试例现在自装 `csrw mtvec, xxx` 和使用手写汇编。当 `set_handler` / `bind_interrupts!` 功能完备后，可考虑：

- 保留这些例子作为"绕过框架的最小验证路径"（独立 baseline）
- 新增 `timer_irq_framework` / `gpio_irq_framework` 测试例，使用 `bind_interrupts!` 框架路径

---

## 四、不变量与约束

### 4.1 跨 crate ABI

```
hisi-riscv-rt (startup.S)
  → call mie0_interrupt_handler  (weak → Rust 侧）
    返回后：
  → csrr a0, mscratch           (a0 = unified frame)
  → call __hisi_irq_epilogue    (hisi-rtos 提供 strong 符号)
  → j __hisi_resume_trap        (hisi_pop_task_context + mret)
```

这根链路保持不变。`set_handler` / `bind_interrupts!` 插入在 `mie0_interrupt_handler` 的**函数体内**，不改变调用约定。

### 4.2 mscratch 约定

```
正常运行:    mscratch = __irq_stack_top__
中断处理中: mscratch = 被打断时的 frame 指针
```

此约定由 `hisi-riscv-rt` 的 `startup.S` 独占管理。任何 Rust 层处理程序不得直接操作 mscratch。

### 4.3 TaskContext 布局

272B `TaskContext` 结构体（`hisi-rtos/src/context.rs`）不被触碰。所有字段偏移保持不变。

### 4.4 浮点寄存器

浮点寄存器（fs0-fs11, ft0-ft11, fa0-fa7, fcsr）在 `hisi_push_task_context` 中全量保存。Rust handler 可以安全使用浮点指令。

---

## 五、风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| `AtomicUsize` 对函数指针的 store 在超标量架构上的可见性 | handler 注册后第一个中断可能看到旧值 | `Ordering::Release`/`Acquire` 配对；fence 在中断入口由 CSR 写隐式完成 |
| 用户错误地在一个 ISR 中既用 `set_handler` 又手写 `#[no_mangle]` | 运行时行为不确定 | 在 API 文档中标注互斥约束；lint 考虑通过编译期检查对称性 |
| `bind_interrupts!` 引入了重复的 `bind_interrupts!` 宏（embassy 版本 vs hisi-hal 版本） | 编译错误或符号双重生成 | 命名约定：hisi-hal 的宏命名为 `hisi_bind_interrupts!` 或保持 `bind_interrupts!` 但明确文档标注 — embassy 绑定在 proc macro 实现上，不会被展开 |
| 函数指针间接调用引入额外 `jalr` 开销 | 1-2 拍的开销 (load + jalr) | 可忽略 — 中断上下文的保存/恢复/换栈/RTOS epilogue 已占用数百拍 |

---

## 六、废弃与与未来迭代

### 6.1 废弃项

- `startup.S` 中手写汇编的 `local_isr_dispatch` 默认实现（将被 `set_handler` 表替代）
- 不在 MI 或 local IRQ 路径上继续用 `push_reg` / `pop_reg` 宏（异常路径和 NMI 路径保留）
- `startup_riscvrt.S` 中的实验模式（`riscv-rt-start-experiment` feature）：当 `set_handler` 框架成熟后标记为废弃

### 6.2 潜在后续工作

- 编译期宏验证：确保 `bind_interrupts!` 中使用的 IRQ 变体与 PAC 枚举一致
- HAL driver 集成：在驱动层提供 `with_handler()` 配置函数（如 `Uart::with_interrupt_handler(pin, handler)`）
- set_handler 并发安全性：评估是否需要 `critical_section` 保护 register，或当前的原子操作是否已足够
