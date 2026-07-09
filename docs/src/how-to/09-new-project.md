# 如何从模板新建一个工程

要从零起一个 WS63/BS2X 应用，用 `cargo generate` 从模板仓库 [hisi-rs-template](https://github.com/hispark-rs/hisi-rs-template) 生成——它帮你配好工具链、链接脚本、依赖和一份 `justfile`，开箱即可构建+烧录。

> 前提：已[安装官方 Rust 工具链](01-install-toolchain.md)；`cargo install cargo-generate`。

## 生成

```bash
cargo generate --git https://github.com/hispark-rs/hisi-rs-template
```

交互式会问两个选项：

- **chip（目标芯片）**：`ws63` / `bs21` / `bs21e` / `bs22` / `bs20`（默认 `ws63`）。BS2X 几个 SKU 在 HAL 里是同一颗实验芯片 target（`chip-bs21`，需 `unstable`），差别只在 L2RAM 大小（bs20=128K，其余 160K，写在 `memory.x`）和 QEMU machine 名。
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
| `just build` | `cargo build -Zbuild-std=core,alloc --release` 编出 ELF |
| `just run` | 在 QEMU 里跑（`cargo run -Zbuild-std=core,alloc --release`） |
| `just patch`（WS63 例外） | build 后 `hisi-fwpkg patch-hash {{elf}}`，用于 `probe-rs run` / embedded-test 这类需要 ELF 元数据的路径 |
| `just image` | build 后 `hisi-fwpkg plan --image-output` 生成 `*.img` + `*.plan.json` |
| `just flash` | 从 plan 读取 `base_addr`，用 `probe-rs download --binary-format bin --base-address` 烧完整 image |
| `just fwpkg` | `hisi-fwpkg pack` 产 `*.fwpkg`（hisiflash/厂商路径） |
| `just clean` | `cargo clean` + 删 img/fwpkg |

烧录配方的前提（构建/run 不需要这些）：`hisi-fwpkg`、[补丁版 probe-rs fork](04-flash-probe-rs.md) + 其 `HiSilicon_WS63.yaml`。`CHIP`/`CHIP_DESC`/`APP_ADDR` 可在命令行覆盖，例如：

```bash
just CHIP_DESC=~/probe-rs/HiSilicon_WS63.yaml flash
```

## 第一次构建 + 烧录

WS63：

```bash
cd my-app
just build          # 编出 release ELF
just image          # hisi-fwpkg plan 生成完整 image + plan
just flash          # probe-rs 按 plan.base_addr 烧裸 bin image
```

> `just flash` 依次做了这些事：先 `hisi-fwpkg plan --image-output`，再从 `*.plan.json` 读取 `base_addr`，最后 `probe-rs download --binary-format bin --base-address ... *.img` 写入完整 image。

BS2X：

```bash
cd my-app
just build          # 编出 release ELF
just image          # hisi-fwpkg plan 生成完整 image + plan
just flash          # probe-rs download 把 *.img 烧进 app 分区并复位
```

烧 BS2X 时把 `CHIP`/`APP_ADDR` 调成对应值（BS2X 基址 `0x00090000`，**尚未 HIL 验证**，先对照分区表确认）。

## 之后

- 不想用 `just`、想理解每一步 → [如何打包镜像](03-package-image.md) + [如何用 probe-rs 烧录](04-flash-probe-rs.md)。
- 想让 `cargo run` 直接烧真机 → [如何用硬件 runner](06-hardware-runner.md)。
- 要加自己的外设驱动 → [如何新增一个外设驱动](10-add-driver.md)。
