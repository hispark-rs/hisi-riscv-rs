# WS53 Rust 指令缓存模型夹具

只用于已经验证的 QEMU `instruction-experiment`，**不是实板 RAM/HIL 入口**。
Rust 镜像所有代码、常量、栈和故障路径均在 MCU_MODE_1 的应用 RAM，加载段没有 Flash。
宿主另在模拟 Flash `0x4ff000` 安装第一方小函数；只在缓存配置返回 RAM 后调用它。
不执行厂商 ROM helper、Flash 烧录或外设初始化。

固件先检查 `.data/.bss`、MIE 与栈，发布第一阶段并等待显式宿主握手。
运行时启动本身执行一次 `fence.i`，单独记录；缓存接口不得在握手前调用。
启用 `qemu-only` 只表达测试意图，握手也不是硬件所有权证明，均不授予实板准入。

## 构建与执行

在父工程目录运行，锁文件位于 WS53 独立 workspace，不与 WS63 feature 合并：

```bash
uv run --script scripts/ws53_cache_contract_build.py --output target/cache-contract-build1
uv run --script scripts/test-ws53-cache-contract-qemu.py \
  --qemu /path/to/qemu-system-riscv32 \
  --model-source /path/to/hisi-riscv-qemu \
  --elf target/cache-contract-build1/firmware.elf \
  --output target/cache-contract-run1.json
```

模型必须匹配本轮具名证据的二进制SHA
`20e0b9299fe7222fd4b4c83c368ba53fc27b512c5f8b34aeba2f523e2bee6d71`。
`--model-source` 可指该模型源码仓库，或封存模型证据中的 `source/`，三个传输工具
按固定校验值检查后才加载。旧的读零/忽略写模型不会被准入。

普通构建不需要 Python 或 SDK；开发者也可直接执行：

```bash
cargo +nightly-2026-07-09 build --locked --offline --release \
  --manifest-path examples/ws53/cache_contract/Cargo.toml --features qemu-only \
  --target riscv32imfc-unknown-none-elf -Zbuild-std=core,alloc
```

省略 `qemu-only` 编译失败。完整生成来源检查额外记录依赖闭包、WS53锁文件、Cargo
配置、工具链/rust-src和构建前后文件值；它是本地可追溯证据，不是独立密闭构建证明。

## 实际执行的内容

相同 ELF 两次模型复位执行。每轮使用实际链接的
`hisi_riscv_rt::ws53_cache::configure_instruction_cache`，完成：

1. 32 KiB、4行配置，观察原始/读回值并执行受控 Flash 函数。
2. 宿主改写 backing 后仍执行旧指令；重复配置返回 AlreadyEnabled，旧指令保持。
3. 重新配置使新指令可见；再切至 8 KiB/1行，最后恢复32 KiB/4行配置。
4. 错误握手在调用前拒绝；API入口故障注入必须在真正链接的 API 地址捕获异常。
5. unknown/historical model 在真实 API 的 ICCTL/APREFI 读取处产生精确、具名拒绝。
6. 整机模型复位后诊断周期和缓存状态清除，再重新加载并运行。

其中“清ICEN以准备重新配置”是夹具中的显式模型脚手架，**不是已经交付的 Rust
cache-disable 驱动**。成功返回只是数值与模型指令可见性，不证明物理维护完成、
D-cache/PMP/共享master交接、DMA coherence、总线 drain 或精确定时。
固件使用私有 RAM 无栈失败向量，不测试运行时的通用异常分发或硬件 NMI。

数据/指令几何、Flash cacheable、顺序预取与同步维护仍沿用明确的模型假设。
实板RAM启用、Flash运行、冷启动和约1.58ms延后的因果关系均待独立验收。
