# 如何用 probe-rs 烧录到真机

probe-rs 在本项目里只承担**传输/写 flash**：它不解析 HiSilicon header/hash/body 规则。普通 smoke/download 路径统一先让 `hisi-fwpkg plan` 生成完整 flash image，再用 probe-rs 的通用裸 bin 下载能力写入：

```bash
probe-rs download --binary-format bin --base-address <plan.base_addr> <plan.image>
```

> 用串口/YMODEM 而不是 SWD/JTAG 探针的话，走[厂商 hisiflash 路径](05-flash-hisiflash.md)。

## 前提：补丁版 probe-rs fork

上游 probe-rs 还没有 WS63 target，也没有 `ws63-sfc` flash 算法。必须装补丁版 fork：

```bash
cargo install --git https://github.com/hispark-rs/probe-rs \
    --branch add-hisilicon-ws63-bs21-hil-baseline probe-rs-tools
```

同时需要该 fork 随附的芯片描述 `HiSilicon_WS63.yaml`（在 fork 仓库 `probe-rs/targets/HiSilicon_WS63.yaml`）。烧录时用 `--chip-description-path` 指向它。

## 手动三步

```bash
# 1. 构建 ELF
cargo build -p blinky --release

# 2. 由 hisi-fwpkg 生成 plan + 完整 flash image
hisi-fwpkg plan \
    target/riscv32imfc-unknown-none-elf/release/blinky \
    --chip ws63 \
    --image-output blinky.img \
    > blinky.plan.json

# 3. probe-rs 只按裸 bin 写入 plan 指定的基址
BASE_ADDR=$(uv run scripts/read-flash-plan-base.py blinky.plan.json)
probe-rs download --chip WS63 \
    --chip-description-path HiSilicon_WS63.yaml \
    --speed 2000 \
    --verify \
    --binary-format bin \
    --base-address "$BASE_ADDR" \
    blinky.img
```

复位策略和镜像语义分开处理。WS63 HIL smoke 使用 J-Link nRST 或脚本封装的 reset/capture 路径；不要把 reset 行为写进镜像格式规则。

## 各芯片默认 app 基址

这些默认值由 `hisi-fwpkg plan` 输出；手动表只作参考。自定义分区表时用 `hisi-fwpkg plan --app-addr <ADDR>` 覆盖。

| 芯片 | app 分区基址 |
| --- | --- |
| WS63 | `0x00230000` |
| BS2X（bs21/bs20…） | `0x00090000` |

## 用脚本

`hil/flash.sh` 默认 `METHOD=probe-rs`，内部调用 `hil/pack.sh` 生成 `.img` 和 `.plan.json`，再从 plan 读取 `base_addr`：

```bash
PROBE_RS_YAML=/path/HiSilicon_WS63.yaml hil/flash.sh blinky
```

可用环境变量：

| 变量 | 含义 | 默认 |
| --- | --- | --- |
| `PROBE_RS_YAML` | fork 的芯片描述 yaml（必填） | - |
| `CHIP` | `probe-rs --chip` 值 | `WS63` |
| `CHIP_KIND` | `ws63`/`bs21`，传给 `hisi-fwpkg plan` | `ws63` |
| `BASE_ADDRESS` | 可选 app 分区覆盖值，会传给 plan | 未设则用 chip 默认 |
| `PROBE_RS` | probe-rs 二进制 | `probe-rs` |
| `PROBE_SPEED` | 调试传输时钟，单位 kHz | `2000` |

WS63 当前默认使用 `2 MHz`。`4 MHz` 可缩短小镜像传输，但在长镜像连续传输中观察到过 DAP NoAck，因而不作为 HIL 默认值；调整速度不能替代完整校验和复位后的启动 marker。

`hil/flash.sh` 和 `hil/cargo-run-hw.sh` 始终传入 `--verify`，写入后完整回读计划镜像。不要为了缩短 HIL 时间删除校验；flashboot 的 body hash 只能证明它读取的 body，不能代替主机对完整计划镜像的回读。

### WS63 下载性能基线

以下数据来自同一块真实 WS63、同一份 `656584` 字节 RF plan image，并保留完整回读校验：

| 路径 | 三次或稳定测量 | 说明 |
| --- | --- | --- |
| 4 KiB sector erase、400 kHz | `202.72 / 206.20 / 201.04 s` | 约 161 次 erase 调用 |
| 64 KiB block erase、2 MHz | `79.71 / 79.98 / 79.72 s` | 约 11 次 erase、11 次 program |
| 上述路径 + WS63 显式 repeated-DMI batch `64` | `29.65 / 29.45 / 29.49 s` | 两个 64 KiB page buffer，完整 verify |

repeated-DMI 不是所有 `ArmWithRiscv` target 的通用默认值。probe-rs 默认仍逐字写；只有 target 明确声明 `dmi_repeated_write_batch_size` 才启用，并在每批后检查 Debug Module 状态。WS63 的 `64` 是经过小 UART 镜像和大 RF 镜像重复 HIL 后的目标能力上限，不能据此改动 RP235x 或其它目标。

若一次激进实验中途失败，SFC/flash 状态可能污染后续结果。此时先用官方完整 fwpkg 恢复已知基线，再重新测量；不能把污染状态下的 verify 失败归因于新 batch 值。

## embedded-test / probe-rs run 例外

`probe-rs run <elf>` 和 embedded-test 需要 ELF 符号、测试元数据、semihosting 信息，因此暂时仍走：

```bash
hisi-fwpkg patch-hash <test-elf>
probe-rs run --chip WS63 --chip-description-path HiSilicon_WS63.yaml <test-elf>
```

这条路径的风险边界是：probe-rs 仍会按 ELF 段写入 flash。普通 smoke/download 已迁移到 plan image；当前 probe-rs CLI 中 `run` 明确是 flash-and-run，`attach` 只 attach RTT/日志，不提供 embedded-test 的 test-discovery/test-dispatch 运行形态。因此 embedded-test 暂时保留 ELF `run` 路径，直到上游/本 fork 有可靠的 “download bin + run tests without reflashing” 能力。

## 排错

- **`'probe-rs' not found` 或 `chip 'WS63' not found`**：装的是上游 probe-rs，不是补丁版 fork。
- **`PROBE_RS_YAML not found`**：忘了给 yaml 路径，或路径错。
- **`Flash Init Fail`**：在 WS63 flashboot 输出里常见，很多情况下非致命；看后续是否进入 app 和 UART marker。
- **download 成功但 reset 后没反应**：先看 plan 的 `body_range` / `code_area_hash`，确认烧的是 `hisi-fwpkg plan --image-output` 产物，而不是未经 plan 展开的 ELF/body。
- **hash/VE 类错误**：通常说明 header 里的 body hash 和 flashboot 实际连续读取的 body 不一致。不要在 probe-rs 里补格式规则，回到 `hisi-fwpkg plan` 诊断 image、body range、gap 填充值和写入边界。

要边烧边看 UART、或让 `cargo run` 直接烧真机，见[如何用硬件 runner 让 cargo run 烧真机](06-hardware-runner.md)；要 attach 调试/读内存见[如何用 probe-rs 调试与读内存](08-debug-probe-rs.md)。
