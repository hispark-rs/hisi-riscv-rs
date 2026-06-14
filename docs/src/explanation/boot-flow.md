# 启动流程：mask ROM → flashboot → app

这一篇回答一个看似简单、却让很多人第一次烧 WS63 时困惑的问题：**为什么我 `cargo build`
出来的那个 ELF，直接烧上去不会跑？** 答案藏在一整条引导链里——从上电的第一条指令，到你的
`main()` 拿到控制权，中间隔着好几道关卡，每一道都对镜像格式有要求。理解这条链，
你就理解了为什么必须"打包 + 烧到特定地址"（操作步骤见
[打包成可启动镜像](../how-to/package-image.md) 与 [用 probe-rs 烧录](../how-to/flash-probe-rs.md)，
确切地址见 [内存映射](../reference/memory-map.md)）。

## 整条链：四级接力

WS63 上电后，控制权像接力棒一样在四个阶段间传递，每一级都把芯片往"能跑应用"的状态推一步：

```console
上电
  │
  ▼
① mask ROM   @ 0x100000   复位向量 `j 0x100024`，固化在硅片里、不可改
  │           最底层 bring-up，随后把控制交给 flash 里的 loaderboot
  ▼
② loaderboot              一级引导：最早的时钟/外设 bring-up、烧录通道（YMODEM）
  │
  ▼
③ flashboot               二级引导：时钟切到 PLL、SFC 初始化、（可选）校验镜像，
  │           然后【无条件】跳到 app 分区 + 0x300
  ▼
④ app  @ 0x230300         你的 Rust 程序，从 0x300 头部之后的入口开始
              hisi-riscv-rt 的启动代码接管 → 最终调用 main()
```

每一级"为什么存在"都不一样：mask ROM 解决"硅片上电后第一条指令从哪来"，
loaderboot/flashboot 解决"flash 里的东西怎么被搬起来跑、镜像合不合法"，
而 app 这一级才是你的代码。前三级里有两级（mask ROM、app ROM）是**厂商固化的 ROM**——
它们的真实内容是从硅片上读出来的 dump，**专有、仅本地可见**，不会进仓库；
我们对它们的理解来自对照 fbb_ws63 C SDK 和实测行为。

## ① mask ROM：硅片里固化的第一步

复位时 PC 落在 mask ROM 的 `0x100000`，那里是一条 `j 0x100024`——跳过最前面几个字的头部，
进到真正的 bring-up 代码。这段代码无法修改（它就是硅片的一部分），职责是把芯片从"刚上电、
什么都没配"的状态拉到"能从 flash 取下一级"的状态。除了 `0x100000` 的 mask ROM，
还有一块 **app ROM @ 0x109000**——厂商固化的运行时支持例程（C SDK 的某些底层函数会调到它）。
对纯 Rust 的裸机应用来说，app ROM 基本不在路径上；但理解连接性（RF blob）时它很关键，
因为厂商协议栈会跳进这些 ROM 地址——这正是 blob 难以脱离真硅片的原因之一
（见 [ws63-RF 深入文档](components/ws63-RF.md)）。

## ②③ loaderboot 与 flashboot：把镜像搬起来、跳进去

loaderboot 是一级引导，flashboot 是二级引导。对"跑一个 Rust 应用"这件事，
**最关键的是 flashboot 的最后一跳**：

> flashboot **无条件**跳到 `app 分区起址 + 0x300`。WS63 的 app 分区在 flash 的
> `0x230000`，所以入口固定是 **`0x230300`**。

注意"无条件"三个字——flashboot **不去解析 ELF 头、不去找 entry point、不做任何重定位**。
它只是把 PC 设到 `0x230300` 然后一跳了事。这就直接解释了下一节那个核心问题。

仓库里有一个**实验性、学习用途**的 Rust 版 flashboot（`chips/ws63/flashboot`），
它对照原厂 `flashboot_ws63` 重写了这条流程：汇编启动（PMP 清零、`mtvec`、开 FPU、清 BSS）、
时钟从 TCXO 切到 PLL、SFC 四线读初始化、镜像头边界校验 + 软件 SHA-256 完整性校验，
最后 `transmute` 到 `addr + 0x300` 跳进去。它**有意不依赖 PAC/HAL**（裸 MMIO），
以免链接进第二份 PAC。生产上不该用它——生产应复用原厂 flashboot，
它有真实签名验签、A/B 槽、FOTA、解压。详见 [flashboot 深入文档](components/ws63-flashboot.md)。

## 为什么 0x300 头部必须存在——以及为什么裸 ELF 不会启动

把上面两件事拼起来，答案就清楚了：

- flashboot **无条件**跳到 `app 分区 + 0x300`；
- 它**不解析 ELF**。

所以 app 分区开头那 **0x300（768）字节必须是一段 HiSilicon 镜像头**——一个 0x100 字节的
KeyArea（签名/密钥区）加一个 0x200 字节的 CodeInfo（含 body 长度、body 的 SHA-256 等）。
flashboot 跳到 `+0x300` 时，正好落在这段头部**之后**、也就是你程序真正的第一条指令上。

如果你把 `cargo build` 出来的**裸 ELF 直接写到 `0x230000`**，会发生什么？
flashboot 照样无条件跳到 `0x230300`——但那里现在是 ELF 文件里偏移 0x300 处的某段**数据
或节内容**，不是入口指令。PC 落在一堆并非代码的字节上（或者 SRAM 残留），于是跑飞。
你的程序明明被烧进去了，却一条指令都没执行到。

这就是为什么必须用 [`hisi-fwpkg`](../how-to/package-image.md) 打包：它把 ELF/bin 转成
"0x300 头部 + body"的镜像，把入口对齐到 `+0x300`，并把 body 的 SHA-256 填进 CodeInfo。
头部各字段的精确布局见 [应用镜像格式与签名](../reference/image-format.md)。

## XIP：app 直接在 flash 里执行

还有一个值得理解的点：WS63 的应用是 **XIP（execute in place）** 的——代码段不被搬进 RAM，
而是直接从 flash 的 XIP 窗口（映射在 `0x200000` 区域）取指执行。app 分区 `0x230000`
就落在这个窗口里。这意味着 flashboot 跳进 `0x230300` 后，CPU 是直接对着 flash 取指的，
SFC（flash 控制器）必须已经被初始化成可读状态——这正是 flashboot 在跳转前要做 SFC 四线读
初始化的原因。

## ④ app：hisi-riscv-rt 接管

控制权落到 `0x230300` 你的程序入口后，并不是直接进 `main()`，而是先经过
[hisi-riscv-rt](components/hisi-riscv-rt.md) 的启动序列。这段代码做的是每个裸机 Rust 程序
都需要、但又必须按 WS63 实际情况定制的事，大致顺序：

1. **PMP 清零**——把物理内存保护配成不挡路（否则后续访问可能陷入）；
2. **设置 `mtvec`**——安装中断/异常向量基址（向量化模式）；
3. **初始化 `gp` / `sp`**——`gp` 用于 linker relaxation 的全局指针寻址，`sp` 指向栈顶；
4. **栈染色（stack paint）**——往栈区填已知图案，便于事后测高水位 / 检测溢出；
5. **`runtime_init`**——把 `.data` 从 flash 拷到 RAM、清 `.bss`，让静态变量就位；
6. **调用 `main()`**——到这里你的代码才真正开始跑。

这套序列为什么不能省、为什么 `gp`/`sp`/PMP 这些必须由 rt 而不是应用来做，
属于 rt 这一层的职责；它的链接脚本（`memory.x` / `layout.ld`）如何把段摆到正确地址、
又如何把脚本传播给下游的 bin，见 [hisi-riscv-rt 深入文档](components/hisi-riscv-rt.md)。

## 与 QEMU 的差别：为什么 QEMU 里裸 ELF 反而能跑

一个会让人困惑的对照：在 [QEMU](qemu-model.md) 里，你**直接** `-kernel blinky.elf`
就能跑，根本不需要 0x300 头部、不需要 flashboot。这不矛盾——QEMU 用 `load_elf()`
**解析 ELF 并按 ELF 的物理地址落段**，再把复位向量设成 ELF 的 entry。也就是说 QEMU
替你做了"理解 ELF、跳到正确入口"这件 flashboot **不做**的事。

所以记住这条分界：**QEMU 跑的是裸 ELF（无头部、无 flashboot、时钟取标称值）；真硅片跑的是
带 0x300 头部、烧到 app 分区、经 flashboot 跳入的镜像。** 这正是 QEMU 能验证逻辑、却验证不了
"镜像格式 / 引导链 / 真实时钟"的根本原因——详见 [QEMU 模型](qemu-model.md) 和
[HIL 框架](hil-framework.md)。
