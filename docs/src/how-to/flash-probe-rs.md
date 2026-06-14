# 如何用 probe-rs 烧录到真机

这是 **2026-06-14 在真实 WS63 硅片上跑通的验证主路径**（`blinky` 上电启动 + 翻转 GPIO0）。流程三步：把 ELF 打包成 0x300 头镜像，用 `probe-rs download` 写进 XIP flash 的 app 分区，再 `probe-rs reset` 复位运行。

> 用串口/YMODEM 而不是 SWD/JTAG 探针的话，走[厂商 hisiflash 路径](flash-hisiflash.md)。

## 前提：补丁版 probe-rs fork（必须）

**上游 probe-rs 还没有 WS63 target，也没有 `ws63-sfc` flash 算法**，用 mainline 烧不了。必须装补丁版 fork：

```bash
cargo install --git https://github.com/hispark-rs/probe-rs \
    --branch add-hisilicon-ws63-bs21 probe-rs-tools
```

同时需要该 fork 随附的芯片描述 **`HiSilicon_WS63.yaml`**（在 fork 仓库 `probe-rs/targets/HiSilicon_WS63.yaml`）。烧录时用 `--chip-description-path` 指向它。该端口的来历见[probe-rs 端口说明](../explanation/components/index.md)。

## 三步走（手动）

```bash
# 1. 打包成 0x300 头镜像（见「如何打包镜像」）
hisi-fwpkg image -o app.img \
    target/riscv32imfc-unknown-none-elf/release/blinky

# 2. 下载到 app 分区（WS63 基址 0x00230000）
probe-rs download --chip WS63 \
    --chip-description-path HiSilicon_WS63.yaml \
    --binary-format bin --base-address 0x00230000 app.img

# 3. 复位运行
probe-rs reset --chip WS63 --chip-description-path HiSilicon_WS63.yaml
```

`--binary-format bin` + `--base-address` 是关键：镜像是裸二进制（不是 ELF），要按绝对 flash 地址落位。

## 各芯片基址

| 芯片 | app 分区基址 |
| --- | --- |
| WS63 | `0x00230000` |
| BS2X（bs21/bs20…） | `0x00090000` |

> BS2X 基址来自 `hisi-fwpkg` 的 `Chip::Bs21` 默认值，**尚未 HIL 验证**——烧 BS2X 前对照你的 fbb_bs2x 分区表确认。自定义分区表时用 `--base-address` 覆盖。

## 用脚本一把梭

`hil/flash.sh` 默认 `METHOD=probe-rs`，封装了打包 + download + reset：

```bash
PROBE_RS_YAML=/path/HiSilicon_WS63.yaml hil/flash.sh blinky
```

可用环境变量：

| 变量 | 含义 | 默认 |
| --- | --- | --- |
| `PROBE_RS_YAML` | fork 的芯片描述 yaml（**必填**） | — |
| `CHIP` | `probe-rs --chip` 值 | `WS63` |
| `CHIP_KIND` | `ws63`/`bs21`（选默认 app 基址） | `ws63` |
| `BASE_ADDRESS` | app 分区基址 | ws63 `0x00230000` / bs21 `0x00090000` |
| `PROBE_RS` | probe-rs 二进制 | `probe-rs` |

如要直接用本地编译出来的 fork：`PROBE_RS=/home/.../probe-rs/target/debug/probe-rs`。

## 排错

- **`'probe-rs' not found` 或 `chip 'WS63' not found`**：装的是上游 probe-rs，不是补丁版 fork。重装上面那条 `--branch add-hisilicon-ws63-bs21`。
- **`PROBE_RS_YAML not found`**：忘了给 yaml 路径，或路径错。yaml 在 fork 仓库 `probe-rs/targets/HiSilicon_WS63.yaml`。
- **`"Flash Init Fail"` 之类的提示**：在本端口里**通常非致命**——download 仍会继续并成功。先看最终是否 `Finished`/写入成功，再决定是否当真问题。
- **写入卡住 / 校验失败**：很可能是 flash **block protect（块保护）**没解。先确认 app 分区不在保护区（厂商工具或 SFC 寄存器层面解保护），再重试。
- **download 成功但 reset 后没反应**：确认烧的是 `image`（0x300 头镜像）而不是裸 ELF/bin——裸文件复位后 PC 落在头区，不会进你的程序。

## 之后

要边烧边看 UART、或让 `cargo run` 直接烧真机，见[如何用硬件 runner 让 cargo run 烧真机](hardware-runner.md)；要 attach 调试/读内存见[如何用 probe-rs 调试与读内存](debug-probe-rs.md)。
