# 如何用 probe-rs 调试与读内存

烧录之外，[补丁版 probe-rs fork](flash-probe-rs.md) 还能 attach 上去读内存、读 CSR、复位到指定状态、下硬件断点。本篇是真机诊断的常用招式。

> 全部命令都需要 fork（`--branch add-hisilicon-ws63-bs21`）+ 其 `HiSilicon_WS63.yaml`。下面为简洁省略了 `--chip-description-path HiSilicon_WS63.yaml`，实跑时按需补上（或用 `PROBE_RS_YAML`/`--chip-description-path`）。

## 读内存 / flash

```bash
# 读 app 镜像头开头 16 个 word（验证 0x300 头烧对没）
probe-rs read --chip WS63 b32 0x00230000 16

# 读 app 入口前几条指令
probe-rs read --chip WS63 b32 0x00230300 8
```

`read <宽度> <地址> <个数>`，宽度 `b8`/`b32` 等。地址直接给绝对物理地址（内存映射见[内存映射](../reference/memory-map.md)）。

## 复位行为：reset_and_halt 落在复位向量

本端口修了 `resethaltreq`，所以 **`reset_and_halt` 现在真的停在复位向量 `0x100000`**（而不是任意位置）。这让「复位后第一条指令开始单步」成为可能：

```bash
probe-rs reset --chip WS63                 # 复位并运行
# attach + halt 在复位向量（GUI/脚本里用 reset_and_halt）
```

复位后 core 从 mask ROM（`0x100000`）起跑，ROM 再跳 flashboot、flashboot 跳 app（`0x230300`）。整条链见[启动流程](../explanation/boot-flow.md)。

## 读 CSR

attach 后可读 RISC-V CSR（`mstatus`/`mepc`/`mcause`/`mtvec`…）定位 trap：

```bash
# 在 halt 状态下读（具体子命令依 fork 版本，常见为 probe-rs read 的 CSR 形式或 GUI）
probe-rs read --chip WS63 b32 0x00230300 4   # 读应用代码确认在跑你的程序
```

> `mcause`/`mepc` 对排「跑飞到 ROM」最有用：若 halt 时 PC 在 `0x10xxxx` 区间，说明根本没进 app（多半是 0x300 头没烧对，见[排错](flash-probe-rs.md#排错)）。

## 抓住 app 入口：在 0x230300 下硬件断点

复位后直接 halt 经常**停在 mask ROM 里**，而不是你的程序——因为 ROM/flashboot 要先跑一段。要抓到「应用刚开始执行」的那一刻，**在 app 入口 `0x230300` 下一个硬件断点，再复位运行**，core 会停在你的第一条指令而不是 ROM：

```text
设硬件断点 @ 0x230300  →  reset run  →  命中断点（已在 app 入口）
```

这正是 HIL 诊断里 `examples/trapdump.rs` 那类「trap dump」模式的做法：上电后在 app 入口设硬件断点，命中后 dump 寄存器/CSR/栈，确保你看到的是**应用**状态而不是落在 mask ROM 里的假象。把它当成「真机版 panic backtrace」——QEMU 给不了真实时序下的现场。

## Dump ROM

mask ROM 在 `0x100000`，可整段读出来离线分析（启动早期行为、ROM 边界，见[启动流程](../explanation/boot-flow.md)）：

```bash
# 读 ROM 头部若干 word（按需扩大个数 / 写文件留存）
probe-rs read --chip WS63 b32 0x00100000 64
```

> ROM 是 mask ROM——只读、不可改。dump 出来是为读懂启动链和定位「PC 卡在 ROM」的问题；mask ROM + SFC 是 QEMU 复刻不了的两处真机边界。

## 排错

- **attach 不上 / 找不到芯片**：装的是上游 probe-rs 不是 fork；或没给 yaml。见[用 probe-rs 烧录的排错](flash-probe-rs.md#排错)。
- **halt 后 PC 一直在 `0x10xxxx`**：还在 mask ROM，没进 app——用上面的「app 入口硬件断点」抓应用，并确认烧的是 0x300 头镜像。
- **`"Flash Init Fail"` 类提示**：本端口里通常非致命，不影响 `read`/`reset`。
