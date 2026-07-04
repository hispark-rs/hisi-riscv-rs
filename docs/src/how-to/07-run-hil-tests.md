# 如何运行 HIL 测试

本仓库现在有两条 HIL（hardware-in-the-loop）轨道：

- **HAL 驱动级 HIL**：`embedded-test` + semihosting，经 `probe-rs run` 在真 WS63 上逐个运行 `#[test]`。这是 stable API 的证据线。
- **示例级 smoke**：烧录常规 UART smoke 子集、读 UART、grep 标记串。它验证完整示例镜像与 QEMU smoke 的真机一致性；无 UART 的 `blinky` 和需要半主机的示例不在裸 UART smoke 子集里。

HIL 框架背景见 [HIL 测试框架](../explanation/07-hil-framework.md)；runner 和脚本变量见
[HIL 脚本与 runner 环境变量](../reference/07-hil-markers.md)；示例标记串事实源见
[示例目录与验证标记串](../reference/02-examples.md)。

> 前提：补丁版 `probe-rs` fork 和 `hisi-fwpkg` 已安装；WS63 的 `HiSilicon_WS63.yaml` 可用。
> probe-rs 烧录路径见[用 probe-rs 烧录](04-flash-probe-rs.md)。

## 跑 HAL 驱动级 embedded-test

这是默认 stable API 毕业要看的真机测试。命令里的 runner 只作用于这一次 `cargo test`：

```bash
PROBE_YAML=/path/HiSilicon_WS63.yaml \
CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/embedded-test-runner.sh \
cargo test -p hisi-riscv-hal \
    --no-default-features --features chip-ws63,rt \
    --target riscv32imfc-unknown-none-elf \
    --test hil
```

只构建测试 ELF、不烧板：

```bash
cargo test -p hisi-riscv-hal \
    --no-default-features --features chip-ws63,rt \
    --target riscv32imfc-unknown-none-elf \
    --test hil --no-run
```

传给 `embedded-test` 的过滤参数放在 `--` 后面：

```bash
PROBE_YAML=/path/HiSilicon_WS63.yaml \
CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/embedded-test-runner.sh \
cargo test -p hisi-riscv-hal \
    --no-default-features --features chip-ws63,rt \
    --target riscv32imfc-unknown-none-elf \
    --test hil -- gpio_output_readback
```

`hil/embedded-test-runner.sh` 会先对测试 ELF 执行 `hisi-fwpkg patch-hash`，再调用补丁版
`probe-rs run --chip WS63 ... <elf>`。`embedded-test` 自带入口、panic handler 与 semihosting
测试调度；测试结果以 libtest 兼容格式回到 `cargo test`。

## 跑跨切面 tests-hil

`tests-hil` 是独立 workspace member，覆盖 CPU/PAC/critical-section 这类不属于单个 HAL 驱动的冒烟事实：

```bash
PROBE_YAML=/path/HiSilicon_WS63.yaml \
CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/embedded-test-runner.sh \
cargo test -p tests-hil --target riscv32imfc-unknown-none-elf
```

它不在 `default-members` 里，普通 `cargo build` 不会拉 `embedded-test`。

## 跑示例级 UART smoke

CI 等价入口是 `.agents/skills/hil-smoke/hil.sh ws63`：它先做 chip/preflight 封装，WS63 全套再委托
`hil/hil-smoke.sh`。后者是 WS63 primitive 脚本，会把常规 UART smoke 子集逐个烧到真机、读串口、
断言它打印了预期 grep 模式：

```bash
PORT=/dev/ttyUSB0 \
PROBE_RS_YAML=/path/HiSilicon_WS63.yaml \
hil/hil-smoke.sh
```

走厂商串口烧录路径时，把 `hil/flash.sh` 的 hisiflash 环境变量带上：

```bash
METHOD=hisiflash PORT=/dev/ttyUSB0 \
    LOADERBOOT=/path/loaderboot.bin ADDRESS=0x230000 \
    hil/hil-smoke.sh
```

`hil-smoke.sh` 额外消费：

| 变量 | 含义 | 默认 |
| --- | --- | --- |
| `PORT` | 板子 UART0（必填） | - |
| `UART_BAUD` | 示例 UART0 波特率（8N1） | `115200` |
| `SETTLE` | 每次烧完读串口的秒数 | `4` |
| `MONITOR` | 自定义“打印原始 UART 到 stdout”的命令 | 直接 `cat $PORT` |

脚本当前检查的 UART smoke 子集和 grep 模式见 [HIL 脚本与 runner 环境变量](../reference/07-hil-markers.md)；
完整示例清单与真机状态见 [示例目录与验证标记串](../reference/02-examples.md)。

## 读懂结果

- `embedded-test` 路径：每个 Rust `#[test]` 以 libtest 风格输出 `ok` / `FAILED`；失败表示对应 HAL/PAC/CPU 事实没有在真板上闭合。
- 示例 smoke 路径：每个 `check` 打印 `PASS: '<pat>' seen` 或 `FAIL`；末行汇总 `HIL SMOKE: PASS` / `FAIL`。
- `probe-rs run` 找不到 chip 或 flash 算法：通常装的是上游 probe-rs，不是 `hispark-rs/probe-rs` 的 `add-hisilicon-ws63-bs21` 分支。
- `embedded-test` 没有任何用例输出：先查 `PROBE_YAML`、`hisi-fwpkg patch-hash`、semihosting 通道和测试 ELF 是否用 `--test hil` 构建。
- UART 标记串没出现但板子像在跑：查 `UART_BAUD`、`SETTLE`、UART0 接线；`spi_loopback` 还需要真机短接 MOSI/MISO。

## CI 与 agent 封装

- `.agents/skills/hil-smoke` 只封装**示例级** UART smoke；当前 CI 真机 runner 只接 WS63。
- `hil/embedded-test-runner.sh` 是 HAL / `tests-hil` 的 on-target test runner。
- `.github/workflows/hil.yml` 是接真板的 self-hosted runner 入口；GitHub-hosted runner 不会运行它。
