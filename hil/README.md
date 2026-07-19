# HIL — 真机在环（hardware-in-the-loop）bring-up

ws63-qemu 已把固件「跑得足够真」做软件在环验证；这一层是它的**真机对应**——板子到位后用它把 ROADMAP
阶段 1（HIL bring-up）跑通，验证 QEMU 证明不了的部分（**真实时钟/时序、真实外设**）。

> ✅ **真机验证（2026-06-14）**：完整的 Rust → flash 流程已在**真实 WS63 硅片**上跑通——
> `blinky` 上电启动并翻转 GPIO0。验证路径见下方「打包 + 烧录」。
> ✅ **HAL 驱动级 HIL**：`hisi-hal/tests/hil.rs` 已在真实 WS63 硅片上通过；
> 精确用例数与 stable API 边界见 `docs/src/reference/10-stable-api.md`。

镜像计划/打包/补哈希用 [`hisi-fwpkg`](https://github.com/hispark-rs/hisi-fwpkg)；烧录有两条已记录的路径：
**probe-rs download/run**（验证主路径，需补丁版 fork）与 **hisiflash YMODEM**（厂商路径）。QEMU 端调试见
`ws63-qemu/scripts/debug.sh`。

WS63 现在有两类 runner 路径，别混在一起：

- **smoke/download 路径（推荐）**：`hisi-fwpkg plan` 生成 `.img` + `.plan.json`，probe-rs 只用
  `download --binary-format bin --base-address <plan.base_addr> <img>` 写完整 image。
- **embedded-test / probe-rs run 例外路径**：测试 runner 需要 ELF 符号、测试元数据和半主机信息，
  所以仍先 `hisi-fwpkg patch-hash <elf>`，再 `probe-rs run <elf>`。

> **多芯片支持**：本文针对 WS63 HIL。BS21/BS2X（BLE/SLE，无 Wi-Fi）也有 QEMU 镜像 + 链接脚本，但真机支持仍待验证（见 `chips/bs2x/guide`）。

## 端到端流程（真机验证 2026-06-14）

```bash
# 1. 用 hisi-riscv 工具链（riscv32imfc，硬浮点）构建——已是默认 target
cargo build -p blinky --release

# 2. 生成完整 flash image + plan
hisi-fwpkg plan target/riscv32imfc-unknown-none-elf/release/blinky \
    --chip ws63 --image-output blinky.img > blinky.plan.json

# 3a. 烧录【验证主路径】：probe-rs 只烧裸 bin image
BASE_ADDR=$(uv run scripts/read-flash-plan-base.py blinky.plan.json)
probe-rs download --chip WS63 --chip-description-path HiSilicon_WS63.yaml \
    --speed 2000 --verify \
    --binary-format bin --base-address "$BASE_ADDR" blinky.img

# 3b. 示例 smoke 可走 hil/flash.sh：它内部执行同样的 plan + download
PROBE_RS_YAML=/path/HiSilicon_WS63.yaml hil/flash.sh blinky

# 3c. 或【厂商路径】：打成 .fwpkg，再用 hisiflash 走 YMODEM 烧录
FWPKG=1 hil/pack.sh blinky              # 额外产出 blinky.fwpkg
hisiflash flash target/.../blinky.fwpkg
```

## 为什么必须打包：0x300 header

**裸 ELF/bin 不会被 flashboot 加载** —— flashboot **无条件**跳到 `app_partition + 0x300`
（app 分区 = WS63 flash `0x230000`，故入口 = `0x230300`）。app 分区开头必须是 0x300 字节的
HiSilicon **image header**，缺了它复位后 PC 落在程序之前的 header 区（或 SRAM 残留），不会进你的程序。

[`hisi-fwpkg`](https://github.com/hispark-rs/hisi-fwpkg) 补上这层。四种相关操作：
- `plan` —— 镜像语义事实源：输出 base address、body range、hash、write chunks，并可写出完整 `.img`。
- `image` —— 兼容子命令：只产 `.img`；新脚本优先用 `plan --image-output`。
- `patch-hash` —— ELF 例外路径：只给 `probe-rs run` / embedded-test 这种需要 ELF 元数据的路径使用。
- `pack` —— 把 image 再包进单分区 fwpkg（V1 容器 + CRC），供厂商 hisiflash 烧录。

```bash
cargo install hisi-fwpkg-cli            # 或 cargo install --path <hisi-fwpkg>/crates/hisi-fwpkg-cli

CHIP=ws63 hil/pack.sh blinky            # -> blinky.img + blinky.plan.json
FWPKG=1   hil/pack.sh blinky            # 额外产出 blinky.fwpkg（hisiflash 路径用）
```

`pack.sh` 自动识别 ELF vs raw bin；`CHIP` 传给 `hisi-fwpkg plan`，`APP_ADDR=` 可覆盖 app 分区地址。
默认产出 `.img` 和 `.plan.json`；`FWPKG=1` 额外产出 `.fwpkg`。

## 烧录路径 A：probe-rs download（验证主路径）

这是 2026-06-14 在真硅片上跑通的路径。**需要补丁版 fork**
[`hispark-rs/probe-rs`](https://github.com/hispark-rs/probe-rs/tree/add-hisilicon-ws63-bs21-hil-baseline)
（branch `add-hisilicon-ws63-bs21-hil-baseline`）——**上游 probe-rs 还没有 WS63 target 与 `ws63-sfc` flash 算法**，
用 mainline probe-rs 烧不了；同时需要该 fork 提供的 `HiSilicon_WS63.yaml` 芯片描述。

`hil/flash.sh`（默认 `METHOD=probe-rs`）会先用 `pack.sh` 产出 `.img` 和 `.plan.json`，再执行：

```bash
PROBE_RS_YAML=/path/HiSilicon_WS63.yaml hil/flash.sh blinky
# 等价于：
probe-rs download --chip WS63 --chip-description-path HiSilicon_WS63.yaml \
    --speed 2000 --verify \
    --binary-format bin --base-address 0x00230000 blinky.img
probe-rs reset    --chip WS63 --chip-description-path HiSilicon_WS63.yaml
```

环境变量：`PROBE_RS_YAML`（必填，fork 的芯片描述）、`CHIP`（默认 WS63）、`BASE_ADDRESS`
（可选覆盖，传给 `hisi-fwpkg plan`）、`PROBE_RS`（二进制名）、`PROBE_SPEED`（默认 `2000` kHz）。
脚本始终启用完整 readback verify；性能数据与 repeated-DMI 的 target capability 边界以
[`docs/src/how-to/04-flash-probe-rs.md`](../docs/src/how-to/04-flash-probe-rs.md#ws63-下载性能基线)为唯一详细记录。

## 烧录路径 B：hisiflash YMODEM（厂商路径）

走串口/YMODEM（@230400）。两个**必须按板子确认**的量（写错可能烧不进 / 烧错位置）：

- **`LOADERBOOT`** —— 厂商 LoaderBoot 二进制，`hisiflash write-program` 会先把它推进去再写程序。
  取自 fbb_ws63 构建产物（`src/output/ws63/.../*loaderboot*.bin`）。
- **`ADDRESS`** —— 程序写入的 flash 偏移（典型 app 分区偏移 `0x230000`）。**对照板子的分区表确认**。

```bash
cargo install hisiflash-cli            # 或 cargo install --path /root/hisiflash/hisiflash-cli

# 直写裸 bin（自己负责 header / 或先 hil/pack.sh 产出带 header 的 .img 喂进去）
METHOD=hisiflash PORT=/dev/ttyUSB0 LOADERBOOT=/path/loaderboot.bin ADDRESS=0x230000 \
    hil/flash.sh blinky

# 或直接烧 fwpkg（boot 链已在板上时，只更新 app 即可）
FWPKG=1 hil/pack.sh blinky
hisiflash info  target/.../blinky.fwpkg   # 静态校验结构（V1 / 分区 / CRC）
hisiflash flash target/.../blinky.fwpkg
```

## 示例级 HIL 冒烟

```bash
sudo apt-get install -y gdb-multiarch  # 真机/QEMU 调试（rust-gdb 驱动它）

# 逐例烧录 + 读 UART + 比对标记，镜像 QEMU smoke-test
PORT=/dev/ttyUSB0 PROBE_RS_YAML=/path/HiSilicon_WS63.yaml hil/hil-smoke.sh
```

环境变量：`PORT`（串口）、`BAUD`（烧录波特，hisiflash 默认 921600）、`UART_BAUD`（例子 UART0 波特，默认
115200）、`PROBE_RS_YAML`（probe-rs 路径）、`LOADERBOOT`/`ADDRESS`/`HISIFLASH`（hisiflash 路径）、
`SETTLE`（每次烧录后读 UART 秒数）。

## 在板跑 `cargo test`（embedded-test + 半主机）

在板测试用 [`embedded-test`](https://github.com/probe-rs/embedded-test) 的测试 harness，由补丁版
probe-rs fork 的 `probe-rs run` 经 **RISC-V 半主机（semihosting）** 逐个用例驱动并把结果
（libtest 兼容）报回 `cargo test`。半主机通道已于 2026-06-14 在真硅片上验证（`semihost_selftest`
打印 PASS + 捕获 SYS_EXIT）。在板测试分两处：

- **`tests-hil`** —— 跨切面 / CPU / PAC 冒烟套件：纯 CPU 的 M/F/CSR 指令不变式，以及 PAC 基址
  结构性地址映射不变式（不属于任何单个 HAL 驱动）。
- **`hisi-hal/tests/hil.rs`** —— HAL **驱动**在板测试（GPIO/TCXO/UART/timer/WDT/PWM/TRNG/eFuse/
  I2C/I2S/LSADC/TSENSOR/peripherals 等 stable 子面）。它们与所测代码同处一 crate，随 HAL 发布与运行，并继承 HAL 的芯片
  门控（HAL standalone 无默认芯片；WS63 显式 `--features chip-ws63,rt`，实验性 `chip-bs21` 经 `--features chip-bs21,unstable`）。在板跑（用 `--test hil`
  只构建这一 embedded-test 集成测试目标——HAL 的主机单测在 `src/*.rs` 的 lib 测试目标里用默认
  libtest harness，而裸机 `riscv32imfc` target 没有 `test`/`std` crate，不加 `--test hil` 的
  裸 `cargo test --target riscv…` 会去构建那个 lib 测试目标并链接失败）。

WS63 测试 ELF 自带 0x300 启动头（通过 `hisi-riscv-rt` 的 `boot-header` feature），runner 只需
`hisi-fwpkg patch-hash` 补头部 body SHA-256 即可启动。embedded-test 自带 `main`
入口（导出为 C 符号 `main`，由 `hisi-riscv-rt` 的 `runtime_init` 调用）和 `#[panic_handler]`，
所以测试文件**不**用 `#[entry]`、也不写 panic handler。

```bash
# 1. HAL 驱动级：仅构建测试 ELF（不上板）
cargo test -p hisi-hal \
    --no-default-features --features chip-ws63,rt \
    --target riscv32imfc-unknown-none-elf \
    --test hil --no-run

# 2. HAL 驱动级：在板跑全部用例
#    （只覆盖这一次；.cargo/config.toml 里 `cargo run` 仍走 QEMU，不受影响）
PROBE_YAML=/path/HiSilicon_WS63.yaml \
CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/embedded-test-runner.sh \
    cargo test -p hisi-hal \
        --no-default-features --features chip-ws63,rt \
        --target riscv32imfc-unknown-none-elf \
        --test hil

# 3. 跨切面 tests-hil：在板跑全部用例
PROBE_YAML=/path/HiSilicon_WS63.yaml \
CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/embedded-test-runner.sh \
    cargo test -p tests-hil --target riscv32imfc-unknown-none-elf
```

runner（`hil/embedded-test-runner.sh`）环境变量（均可选，对齐 `cargo-run-hw.sh`）：
`PROBE_RS`（probe-rs 二进制，需补丁 fork `hispark-rs/probe-rs` branch `add-hisilicon-ws63-bs21-hil-baseline`）、
`PROBE_CHIP`（默认 `WS63`）、`PROBE_YAML`（`--chip-description-path` 的 YAML，默认空=内置库）、
`PROBE_SPEED`（默认 `2000`，单位 kHz）、
`HISI_FWPKG`（默认 `hisi-fwpkg`）。runner 先 `hisi-fwpkg patch-hash <elf>`（原地补头），
再 `probe-rs run --chip WS63 [--chip-description-path YAML] <elf> [embedded-test 参数]`。

用例（自包含、无需跳线，QEMU/裸板皆安全）：`tests-hil` 跨切面套件 = (a) M/F/CSR 指令不变式
（整数乘、ilp32f 硬浮点、mcycle 自增，镜像 `semihost_selftest`）；(b) PAC 基址结构性断言
（GPIO0/UART0/TCXO/I2C0/PWM/WDT/RTC… 窗口未漂移）。HAL 驱动套件
（`hisi-hal/tests/hil.rs`）已在真机通过；具体用例数、stable API 覆盖与不包含项以
`docs/src/reference/10-stable-api.md` 为事实源。

> 注意：`tests-hil` 是 workspace member 但**不在 default-members**，故普通 `cargo build` 不会拉
> embedded-test。HAL 的在板测试是 riscv-only 的 target-gated dev-dep，普通 `cargo build -p
> hisi-hal` 与主机单测（`cargo test --target x86_64`）都不会拉 embedded-test / hisi-riscv-rt。

## Bring-up 清单（按序，每步附预期 + 失败诊断）

| 步 | 验证 | 预期 | 失败诊断 |
|----|------|------|----------|
| 1 | **上电 + flashboot** | 串口有 flashboot/loaderboot 输出 | 检查电源、PWR_ON、串口线/波特、LOADERBOOT |
| 2 | **blinky** | LED 闪（GPIO0）/ 逻辑分析仪见方波 | GPIO 引脚映射、init_output、时钟门控 |
| 3 | **uart_hello** | `Hello from WS63 …` @115200 | **验证 160 MHz 波特基**——波特不对说明 UART 时钟假设错（见 ch8 时钟树） |
| 4 | **timer_irq** | `timer irq #…` 周期到达 | **验证 24 MHz TCXO 定时器时钟**——周期偏 10× 说明时钟仍按 240 MHz 算 |
| 5 | **gpio_irq** | `gpio irq #…`（按键/注入） | 中断接线、LOCI* 使能、触发沿 |
| 6 | **reset_demo** | 复位 + `OK: software reset observed` | GLB_CTL_M(0x4000_2110) / SYS_RST_RECORD_0 |
| 7 | **SPI / I2C** | `spi_loopback`（短接 MOSI-MISO）/ `i2c_scan` | SPI 两级时钟、I2C 24 MHz SCL |
| 8 | **DMA（可选）** | `dma_loopback` | DMA 握手 ID、外设/mem 转移（QEMU 已验证，通用拓展用；首板非门禁） |
| 9 | **连接性（阶段 4/5）** | blob 链接镜像跑通 FRW/HCC → netif | ROM 地址 + 厂商重定位（HIL 专属，QEMU 无法）；仅 WS63 支持（BS2X BLE/SLE 见可行性分析） |

**首板第一目标**：跑通步 3–6 确认**本会话的时钟修复在真硅片上准确**（24 MHz 定时器、160 MHz UART 波特、
SPI/I2C、GPIO/复位中断）——这正是 QEMU 数字验证不了、必须上板验的部分（QEMU 已验证投递逻辑，但无真时钟/时序）。一旦通过，步 7 及阶段 4/5（连接性上板）即可推进。

## 真机调试（JTAG/SWD / 串口）

**QEMU**：用 `ws63-qemu/scripts/debug.sh`（gdbstub）——已验证投递闭环但无真时钟/时序。

**真硅片（WS63）**：探针（J-Link/OpenOCD）+ `gdb-multiarch`，或用 HiSilicon 定制 probe-rs：
- 标准 OpenOCD + gdb：

```bash
# 例：OpenOCD 起 gdbstub 后
RUST_GDB=gdb-multiarch rustup run ws63 rust-gdb \
    -ex 'target remote :3333' \
    target/riscv32imfc-unknown-none-elf/release/blinky
```

- **HiSilicon 定制（RISC-V-DM 后端）**：fork [`hispark-rs/probe-rs`](https://github.com/hispark-rs/probe-rs/tree/add-hisilicon-ws63-bs21-hil-baseline)（branch `add-hisilicon-ws63-bs21-hil-baseline`） 已加 DM-behind-CoreSight mem-AP 适配 + `ws63-sfc` flash 算法。**`probe-rs download` 烧录已于 2026-06-14 真机验证**（上方烧录路径 A）；`probe-rs run --chip ws63` 调试可继续在此基础上推进。上游 probe-rs 暂无 WS63 target / flash 算法。

`rust-gdb` 会自动加载 ws63 工具链的 Rust 美化打印器；JTAG/SWD 引脚见 ws63-guide ch7。

> 状态：**Rust → flash → 启动**主流程（构建 → `hisi-fwpkg plan` → `probe-rs download --binary-format bin` → reset）
> 已于 2026-06-14 真机验证（blinky 启动 + 翻转 GPIO0）；HAL 驱动级 embedded-test HIL 已在真机通过，
> 精确覆盖见 `docs/src/reference/10-stable-api.md`。示例 smoke 与连接性 HIL 继续按板逐项推进；无板时仅可做构建 + 打包（不触碰硬件）。

## 参考

- **ROADMAP**：见 [`ROADMAP.md`](../ROADMAP.md) 阶段 1–2 bring-up 规划 + QEMU 验收标准。
- **真机验证状态**：Rust → flash → 启动主流程已于 2026-06-14 在真 WS63 硅片上跑通（blinky）；HAL 驱动级 embedded-test HIL 已在真机通过，精确覆盖见 `docs/src/reference/10-stable-api.md`。后续真机门禁见上方 bring-up 清单步 3–9。
- **QEMU 验证范围**：时钟树改正、IRQ 投递、DMA 握手已在 ws63-qemu 上验证；真机需验证时序精度、真实外设行为、RF blob 链接。
- **连接性**：WS63 Wi-Fi porting 见 `chips/ws63/rf`；BS2X BLE/SLE 可行性分析见 `chips/bs2x/guide/README.md`。
