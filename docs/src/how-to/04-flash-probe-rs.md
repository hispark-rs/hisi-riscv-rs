# 如何用 probe-rs 烧录到真机

这是 **2026-06-14 在真实 WS63 硅片上跑通的验证主路径**（`blinky` 上电启动 + 翻转 GPIO0）。WS63 用 `hisi-riscv-rt` 的 `boot-header` feature 在**链接期**就把 0x300 HiSilicon 头烤进 ELF，所以裸 ELF 本身就可启动——无需 `hisi-fwpkg image` 那一步、也没有中间 `.img` 文件。流程三步：链接后用 `hisi-fwpkg patch-hash` 就地把 body SHA-256 填进头里（secure-off 仍会校验 hash，只跳过 ECC 签名），用 `probe-rs download` 把 ELF 直接写进 XIP flash 的 app 分区，再 `probe-rs reset` 复位运行。

> 用串口/YMODEM 而不是 SWD/JTAG 探针的话，走[厂商 hisiflash 路径](05-flash-hisiflash.md)。

## 前提：补丁版 probe-rs fork（必须）

**上游 probe-rs 还没有 WS63 target，也没有 `ws63-sfc` flash 算法**，用 mainline 烧不了。必须装补丁版 fork：

```bash
cargo install --git https://github.com/hispark-rs/probe-rs \
    --branch add-hisilicon-ws63-bs21 probe-rs-tools
```

同时需要该 fork 随附的芯片描述 **`HiSilicon_WS63.yaml`**（在 fork 仓库 `probe-rs/targets/HiSilicon_WS63.yaml`）。烧录时用 `--chip-description-path` 指向它。该端口的来历见[probe-rs 端口说明](../explanation/components/00-index.md)。

## 三步走（手动，WS63）

```bash
# 1. 链接后填充 body SHA-256（boot-header 已把 0x300 头烤进 ELF，
#    这一步把头里的 body hash 就地补齐；secure-off 仍校验 hash，只跳过 ECC 签名）
hisi-fwpkg patch-hash \
    target/riscv32imfc-unknown-none-elf/release/blinky

# 2. 直接把 ELF 下载进 app 分区（基址来自 ELF 里 boot-header 的链接地址，
#    无需 --base-address；fork 的 ws63-sfc 算法会按 ELF 段地址落位）
probe-rs download --chip WS63 \
    --chip-description-path HiSilicon_WS63.yaml \
    target/riscv32imfc-unknown-none-elf/release/blinky

# 3. 复位运行
probe-rs reset --chip WS63 --chip-description-path HiSilicon_WS63.yaml
```

关键：WS63 烧的是**带 boot-header 的 ELF 本身**，不是裸二进制，所以不需要 `--binary-format bin` + `--base-address`——`probe-rs` 按 ELF 段地址落位即可。`patch-hash` 是必须的后链接步骤，少了它 flashboot 校验 body hash 不过、不会进你的程序。

> **BS2X（bs21/bs20…）走 route 1：** 还没有链接期 boot-header，要先 `hisi-fwpkg image -o app.img <elf>` 打出 0x300 头镜像（裸二进制），再用 `--binary-format bin --base-address <app 基址>` 把 `.img` 落到 app 分区。下面「各芯片基址」表给的就是 BS2X 这条路要用的基址。

## 各芯片基址（route 1 / BS2X 用）

WS63 走 route 2 烧 ELF，地址由 boot-header 链接进去，**不需要手填基址**。下表是 route 1（`hisi-fwpkg image` + `--base-address`）落 `.img` 时用的 app 分区基址——目前主要给 BS2X，也可作为 WS63 boot-header 链接地址的参考：

| 芯片 | app 分区基址 |
| --- | --- |
| WS63（boot-header 链接地址 / route 1 参考） | `0x00230000` |
| BS2X（bs21/bs20…，route 1） | `0x00090000` |

> BS2X 基址来自 `hisi-fwpkg` 的 `Chip::Bs21` 默认值，**尚未 HIL 验证**——烧 BS2X 前对照你的 fbb_bs2x 分区表确认。自定义分区表时用 `--base-address` 覆盖。

## 用脚本一把梭

`hil/flash.sh` 默认 `METHOD=probe-rs`，封装了 download + reset 一条龙：

```bash
PROBE_RS_YAML=/path/HiSilicon_WS63.yaml hil/flash.sh blinky
```

> 注意：当前 `hil/flash.sh` 仍走 route 1 老流程（内部调 `hil/pack.sh` 跑 `hisi-fwpkg image` 出 `.img`，再 `--binary-format bin --base-address` 落位）——对 BS2X 正确，对 WS63 也仍能跑通（`.img` 与 boot-header ELF 的 body 一致）。WS63 的精简 route 2 推荐路径见上面「三步走」或模板 `justfile` 的 `patch`/`flash` recipe（`hisi-fwpkg patch-hash` + 直接烧 ELF）。

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
- **download 成功但 reset 后没反应**：
  - WS63（route 2）：八成是忘了跑 `hisi-fwpkg patch-hash`——头里 body SHA-256 没填，flashboot 校验 hash 不过，不会进你的程序（secure-off 只跳过 ECC 签名，hash 仍校验，**没有“假签名/dummy 签名”能让它启动**，必须是真实 body hash）。也要确认烧的是带 boot-header 的 ELF，不是 `cargo` 直接产出但没 patch 的 ELF。
  - BS2X（route 1）：确认烧的是 `hisi-fwpkg image` 出的 `.img`（0x300 头镜像）而不是裸 ELF/bin——裸文件复位后 PC 落在头区，不会进你的程序。

## 之后

要边烧边看 UART、或让 `cargo run` 直接烧真机，见[如何用硬件 runner 让 cargo run 烧真机](06-hardware-runner.md)；要 attach 调试/读内存见[如何用 probe-rs 调试与读内存](08-debug-probe-rs.md)。
