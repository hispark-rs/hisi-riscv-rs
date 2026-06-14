# CLI 工具速查

本项目工具的命令参考：`hisi-fwpkg`、补丁版 `probe-rs`、QEMU、`hisiflash`。事实取自 `hisi-fwpkg-cli/src/main.rs`、HIL 脚本、tutorials。

镜像字段布局见 [应用镜像格式](image-format.md)；烧录步骤见 [用 probe-rs 烧录](../how-to/flash-probe-rs.md)、[用 hisiflash 烧录](../how-to/flash-hisiflash.md)。

## `hisi-fwpkg`

把编译产物（ELF 或裸 bin，按 magic 自动识别）打包成 HiSilicon app 镜像 / fwpkg。

```bash
cargo install hisi-fwpkg-cli
```

### `hisi-fwpkg image`（路线 1 / BS2X）

ELF/bin → app 镜像（`0x300` header || body，含真实 body SHA-256）。这是 **BS2X 的路线 1** 产物：BS2X 暂无 link-time `boot-header`，构建后用 `image` 单独生成可启动 `.img`，再烧到 app 分区。

> WS63 走 **路线 2**：`boot-header` feature 已把 `0x300` 头烤进 ELF（链接期），构建后只需 `hisi-fwpkg patch-hash <elf>`（见下）补 body hash，直接烧裸 ELF——不再有中间 `.img`，也不走 `image`。

| 参数 | 说明 |
|------|------|
| `<input>` | 输入 ELF 或裸 `.bin`（位置参数） |
| `-o, --output <PATH>` | 输出镜像路径（必填） |

```bash
# BS2X 路线 1：
hisi-fwpkg image app -o app.img
```

### `hisi-fwpkg patch-hash`（路线 2 / WS63）

WS63 的 **路线 2** post-link 步骤：原地把裸 ELF（已含 link-time `0x300` 头）的 body SHA-256 填回头部。无输出文件、无 `.img`，补好后直接 `probe-rs download <elf>` 烧、`probe-rs run <elf>` 跑。

> 硅片上 flashboot **始终校验 body hash**：即便 efuse `SEC_VERIFY_ENABLE==0`（secure-off）也只跳过 ECC **签名**，不跳过 hash——所以镜像需要 **真实 body hash**，没有任何「dummy 签名」能让它启动。`patch-hash` 正是用来填这个真实 hash 的。

| 参数 | 说明 |
|------|------|
| `<input>` | 输入裸 ELF（含 `boot-header` 烤入的 `0x300` 头，位置参数；原地修改） |

```bash
# WS63 路线 2：
hisi-fwpkg patch-hash blinky
```

### `hisi-fwpkg pack`

上面的镜像再包进单分区 fwpkg（V1 容器 + CRC），供厂商 hisiflash 烧录。

| 参数 | 默认 | 说明 |
|------|------|------|
| `<input>` | — | 输入 ELF 或裸 `.bin`（位置参数） |
| `-o, --output <PATH>` | — | 输出 `.fwpkg` 路径（必填） |
| `-c, --chip <ws63\|bs21>` | `ws63` | 目标芯片（决定 app 分区地址） |
| `--app-addr <ADDR>` | （芯片默认） | 覆盖 app 分区 burn 地址（接受 `0x` 十六进制） |
| `--name <NAME>` | `app` | fwpkg 内分区名 |

```bash
hisi-fwpkg pack blinky -o blinky.fwpkg --chip ws63 --name app
```

## `probe-rs`（补丁版 fork）

**需补丁版 fork** [`hispark-rs/probe-rs`](https://github.com/hispark-rs/probe-rs)（branch `add-hisilicon-ws63-bs21`）——上游 probe-rs 尚无 WS63 target 与 `ws63-sfc` flash 算法。需 fork 提供的 `HiSilicon_WS63.yaml` 芯片描述。

本项目用到的子命令与标志：

| 命令 | 用法 |
|------|------|
| `download`（WS63 / 路线 2） | `probe-rs download --chip WS63 --chip-description-path HiSilicon_WS63.yaml <elf>`（裸 ELF 已含 `0x300` 头 + `patch-hash` 补好的真实 body hash） |
| `download`（BS2X / 路线 1） | `probe-rs download --chip BS21 --chip-description-path HiSilicon_WS63.yaml --binary-format bin --base-address 0x00090000 <app.img>` |
| `run`（WS63 / 路线 2） | `probe-rs run --chip WS63 --chip-description-path HiSilicon_WS63.yaml <elf>`——`just run` 的硅片版，烧+跑+抓 RTT/semihosting |
| `reset` | `probe-rs reset --chip WS63 --chip-description-path HiSilicon_WS63.yaml` |
| `read` | 读内存/外设（调试） |
| `gdb` | 启 GDB stub |
| `debug` | 交互调试 |

| 标志 | 说明 |
|------|------|
| `--chip <NAME>` | 目标芯片（`WS63`；bs21 用 BS21） |
| `--chip-description-path <YAML>` | fork 的 `HiSilicon_WS63.yaml` |
| `--binary-format bin` | **仅路线 1（BS2X `.img`）需要**：输入为裸 bin。WS63 路线 2 直接烧 ELF，不加此标志 |
| `--base-address <ADDR>` | **仅路线 1 需要**：app 分区 flash 地址（bs21 `0x00090000`）。WS63 路线 2 的地址由 ELF 内 `0x300` 头自带，无需此标志 |

调试与读内存细节见 [用 probe-rs 调试与读内存](../how-to/debug-probe-rs.md)。

## QEMU

姊妹仓 [`hisi-riscv-qemu`](https://github.com/hispark-rs/hisi-riscv-qemu) 的 QEMU fork，提供 `-M ws63 / bs21 / bs21e / bs22 / bs20` 机器。软件在环，无需硅片。

```bash
qemu-system-riscv32 -M ws63 -nographic -bios none -kernel <elf>
```

| 标志 | 说明 |
|------|------|
| `-M ws63` | WS63 机器模型（另有 `bs21`/`bs21e`/`bs22`/`bs20`） |
| `-nographic` | 无图形，串口接终端 |
| `-bios none` | 不加载默认固件 |
| `-kernel <elf>` | 加载 ELF |
| `-semihosting` | 启用 RISC-V 半主机（`semihost_selftest` 必需） |
| `-serial mon:stdio` | 串口复用 stdio + monitor |
| `-nic user` | user netdev（SLIRP，`net_ping` 需要；默认即 user） |

QEMU 模型原理见 [QEMU 模型](../explanation/qemu-model.md)。

## `hisiflash`

厂商串口/YMODEM 烧录 CLI（@230400）。

```bash
cargo install hisiflash-cli
```

| 命令 | 用法 |
|------|------|
| `write-program` | `hisiflash write-program --loaderboot <loaderboot.bin> <program.bin> --address 0x230000` |
| `info` | `hisiflash info <out.fwpkg>`（静态校验 V1 / 分区 / CRC 结构） |
| `flash` | `hisiflash flash <out.fwpkg>` |

环境变量：`HISIFLASH_PORT`（串口）、`HISIFLASH_BAUD`（烧录波特，默认 921600）。

## 仓库清单

全部位于 GitHub 组织 [`github.com/hispark-rs`](https://github.com/hispark-rs)（旧 `sanchuanhehe/*` URL 已重定向）。

| 仓库 | 一句话 | URL |
|------|--------|-----|
| `hisi-riscv-rs` | 主 monorepo（crates、examples、guides、SVD 均为子模块） | github.com/hispark-rs/hisi-riscv-rs |
| `hisi-rs-template` | cargo-generate 模板（WS63/BS2X 新工程脚手架） | github.com/hispark-rs/hisi-rs-template |
| `hisi-fwpkg` | app 镜像 / fwpkg 打包工具（`image`/`patch-hash`/`pack`） | github.com/hispark-rs/hisi-fwpkg |
| `probe-rs`（fork） | 补丁版 probe-rs（WS63/BS21 target + ws63-sfc flash 算法） | github.com/hispark-rs/probe-rs（branch `add-hisilicon-ws63-bs21`） |
| `hisi-riscv-rust-toolchain` | 自定义 rustc（riscv32imfc builtin，硬浮点） | github.com/hispark-rs/hisi-riscv-rust-toolchain |
| `hisi-riscv-qemu` | QEMU fork（`-M ws63/bs21/bs21e/bs22/bs20`） | github.com/hispark-rs/hisi-riscv-qemu |
| `hisiflash` | 串口/YMODEM 烧录 CLI | github.com/hispark-rs/hisiflash |
