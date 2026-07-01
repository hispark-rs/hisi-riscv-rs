# 如何从模板新建一个工程

要从零起一个 WS63/BS2X 应用，用 `cargo generate` 从模板仓库 [hisi-rs-template](https://github.com/hispark-rs/hisi-rs-template) 生成——它帮你配好工具链、链接脚本、依赖和一份 `justfile`，开箱即可构建+烧录。

> 前提：已[安装 hisi-riscv 工具链](01-install-toolchain.md)；`cargo install cargo-generate`。

## 生成

```bash
cargo generate --git https://github.com/hispark-rs/hisi-rs-template
```

交互式会问两个选项：

- **chip（目标芯片）**：`ws63` / `bs21` / `bs21e` / `bs22` / `bs20`（默认 `ws63`）。BS2X 几个 SKU 在 HAL 里是同一颗芯片（`chip-bs21`），差别只在 L2RAM 大小（bs20=128K，其余 160K，写在 `memory.x`）和 QEMU machine 名。
- **starter（起步应用）**：`blinky` / `uart_hello` / `async`（默认 `blinky`）。
- 还会问 **app 分区 flash 地址**（WS63 默认 `0x00230000`，BS2X 默认 `0x00090000`）——没有自定义分区表就用默认。

非交互式可一把给定：

```bash
cargo generate --git https://github.com/hispark-rs/hisi-rs-template \
    --name my-app --define chip=ws63 --define starter=blinky --silent
```

> WS63 的内存布局来自 hisi-riscv-rt 自带的链接脚本，所以模板**不**为 WS63 生成 `memory.x`；BS2X 才需要工程级 `memory.x`（模板会带）。

## 生成的 justfile

工程带一个 `justfile`（`cargo install just`），封装了硬件验证过的流程：

| 配方 | 做什么 |
| --- | --- |
| `just build` | `cargo build --release` 编出 ELF |
| `just run` | 在 QEMU 里跑（`cargo run --release`） |
| `just patch`（WS63） | build 后 `hisi-fwpkg patch-hash {{elf}}` 补 body 的 SHA-256（0x300 头已由 `boot-header` feature 在链接期嵌进 ELF，无需 `image` 步骤） |
| `just image`（BS2X） | build 后 `hisi-fwpkg image` 补 0x300 头 → `*.img`（BS2X 暂无链接期 boot-header，仍走 image 路径） |
| `just flash` | WS63：patch 后 `probe-rs download` 直接烧裸 ELF 再 `reset`；BS2X：image 后 `probe-rs download` 把 `*.img` 烧进 app 分区再 `reset` |
| `just fwpkg` | `hisi-fwpkg pack` 产 `*.fwpkg`（hisiflash/厂商路径） |
| `just clean` | `cargo clean` + 删 img/fwpkg |

烧录配方的前提（构建/run 不需要这些）：`hisi-fwpkg`、[补丁版 probe-rs fork](04-flash-probe-rs.md) + 其 `HiSilicon_WS63.yaml`。`CHIP`/`CHIP_DESC`/`APP_ADDR` 可在命令行覆盖，例如：

```bash
just CHIP_DESC=~/probe-rs/HiSilicon_WS63.yaml flash
```

## 第一次构建 + 烧录

WS63（route 2，链接期已带 0x300 头）：

```bash
cd my-app
just build          # 编出 release ELF（boot-header feature 已把 0x300 头嵌进 ELF）
just patch          # hisi-fwpkg patch-hash 补 body 的 SHA-256
just flash          # probe-rs download 直接烧裸 ELF 并复位（需 probe-rs fork + yaml）
```

> `just flash` 依次做了这些事：先 `just patch`（`hisi-fwpkg patch-hash {{elf}}` 把 body 的 SHA-256 填进链接期已嵌好的 0x300 头），再 `probe-rs download --chip WS63 ... {{elf}}` 把这份裸 ELF **直接**烧到芯片（无中间 `.img`），最后 `probe-rs reset` 复位运行。

BS2X（route 1，build 后才补 0x300 头打成 `.img`）：

```bash
cd my-app
just build          # 编出 release ELF
just image          # hisi-fwpkg image 补 0x300 头 → *.img
just flash          # probe-rs download 把 *.img 烧进 app 分区并复位
```

烧 BS2X 时把 `CHIP`/`APP_ADDR` 调成对应值（BS2X 基址 `0x00090000`，**尚未 HIL 验证**，先对照分区表确认）。

## 之后

- 不想用 `just`、想理解每一步 → [如何打包镜像](03-package-image.md) + [如何用 probe-rs 烧录](04-flash-probe-rs.md)。
- 想让 `cargo run` 直接烧真机 → [如何用硬件 runner](06-hardware-runner.md)。
- 要加自己的外设驱动 → [如何新增一个外设驱动](10-add-driver.md)。
