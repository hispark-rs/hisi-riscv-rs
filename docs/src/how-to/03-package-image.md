# 如何打包成可启动镜像（hisi-fwpkg）

裸 ELF/bin 烧进 app 分区**不会启动**。flashboot 会**无条件**跳到 `app 分区 + 0x300`（WS63 上 app 分区 = flash `0x230000`，故入口 = `0x230300`）。所以 app 分区开头必须放一段 0x300 字节的 HiSilicon **镜像头**，后面才是你的代码。镜像头的字段布局见[应用镜像格式与签名](../reference/06-image-format.md)，启动流程见[启动流程](../explanation/02-boot-flow.md)。

补这层 0x300 头有两条路线，按芯片不同：

- **WS63（route 2，当前主路径）**：用 `hisi-riscv-rt` 的 `boot-header` feature，0x300 头在**链接期**就烧进 ELF，裸 ELF 本身即可启动——**不需要** `hisi-fwpkg image`。链接后只需补一步 body SHA-256（`hisi-fwpkg patch-hash`），再用 `probe-rs download/run` 直接烧裸 ELF，**没有中间 `.img`**。详见下面的 [WS63：`boot-header` + `patch-hash`](#ws63boot-header--patch-hash)。
- **BS21/BS2X（route 1）**：尚无链接期 boot-header，仍走 `hisi-fwpkg image -o app.img <elf>`（编译后），再把 `.img` 烧到 app 分区。

> 安装：`cargo install --git https://github.com/hispark-rs/hisi-fwpkg`（或 `cargo install hisi-fwpkg-cli`）。

## WS63：`boot-header` + `patch-hash`

WS63 用 `hisi-riscv-rt` 的 `boot-header` feature，把 0x300 HiSilicon 头在链接期直接放进 ELF，裸 ELF 即可启动。链接后再补一步 body 哈希即可：

```bash
# 编译（boot-header feature 会把 0x300 头烧进 ELF）
cargo build --release

# 补 body SHA-256（就地改写 ELF；secure-off 时 flashboot 仍校验 hash）
hisi-fwpkg patch-hash \
    target/riscv32imfc-unknown-none-elf/release/blinky

# 直接烧裸 ELF（无中间 .img），再复位运行
probe-rs download target/riscv32imfc-unknown-none-elf/release/blinky
probe-rs run      target/riscv32imfc-unknown-none-elf/release/blinky
```

`patch-hash` 只接受一个位置参数 `<ELF>`，**就地**填回 body 的 SHA-256（不产新文件）。注意 `cargo flash` 不适用于 WS63 boot-header——它没有插入这步强制 `patch-hash` 的 runner 槽位，无法保证烧进去的 ELF 带正确 body hash。烧录细节见[如何用 probe-rs 烧录](04-flash-probe-rs.md)，新工程脚手架见 `hisi-rs-template` 的 `justfile`（`patch` / `flash` / `run-hw` recipe）。

## BS2X：`hisi-fwpkg` 的两个子命令：`image` vs `pack`

> **仅 BS2X 走 `image`。** WS63 已改用上面的 `boot-header` + `patch-hash`（route 2），不要再对 WS63 的 ELF 跑 `image`。下面的 `image` 子命令针对 BS21/BS2X（route 1）；`pack` 子命令 WS63/BS2X 都可用（厂商 fwpkg 路径）。

`hisi-fwpkg` 自动从 magic 识别输入是 ELF 还是裸 bin，两个子命令各产一种产物：

| 子命令 | 产物 | 内容 | 谁用 |
| --- | --- | --- | --- |
| `image` | `*.img` | 0x300 HiSilicon 头 ‖ body（含 body 的 SHA-256） | **BS2X probe-rs download 路径**（route 1） |
| `pack` | `*.fwpkg` | 把上面的 image 再包进单分区 fwpkg（V1 容器 + CRC） | 厂商 hisiflash / YMODEM 路径（WS63/BS2X 通用） |

### 产 `*.img`（BS2X probe-rs 路径用）

```bash
hisi-fwpkg image -o blinky.img \
    target/riscv32imfc-unknown-none-elf/release/blinky
```

`image` 只有 `-o/--output <OUTPUT>` 和一个位置参数 `<INPUT>`（ELF 或裸 bin）。app 基址在烧录时由 `probe-rs --base-address` 给（见[如何用 probe-rs 烧录](04-flash-probe-rs.md)），所以 `image` 自身不需要芯片/地址参数。

### 产 `*.fwpkg`（hisiflash 路径用）

```bash
hisi-fwpkg pack -o blinky.fwpkg --chip ws63 \
    target/riscv32imfc-unknown-none-elf/release/blinky
```

`pack` 多几个选项：

- `-c/--chip <ws63|bs21>`（默认 `ws63`）：决定 app 分区基址（**ws63 = 0x230000，bs2x = 0x90000**）。
- `--app-addr <APP_ADDR>`：覆盖 app 分区烧录地址（接受十六进制，如 `0x230000`），自定义分区表时用。
- `--name <NAME>`：fwpkg 里分区名（默认 `app`）。

## 用脚本一把梭

- **WS63（route 2）**：用 `hisi-rs-template` 的 `justfile`——`just patch`（`cargo build` + `hisi-fwpkg patch-hash`）、`just flash`（patch + `probe-rs download/reset`）、`just run-hw`（patch + `probe-rs run`）。
- **BS2X（route 1）**：`hil/pack.sh` 封装了 `image`（+ 可选 `pack`），按示例名解析 ELF：

```bash
CHIP=bs21 hil/pack.sh blinky       # -> examples/.../blinky.img（默认只产 .img）
FWPKG=1   hil/pack.sh blinky       # 额外再产一个 blinky.fwpkg
```

`CHIP` 决定 app 基址（`APP_ADDR=` 可覆盖），脚本跑完会把两条烧录命令（probe-rs / hisiflash）打印出来供复制。`pack`/fwpkg（厂商 hisiflash 路径）对 WS63 同样可用。

## 关于签名：本片不需要真签名（但需要真实 body hash）

镜像头里有签名字段，但**开发芯片 secure boot 是关的**（efuse `SEC_VERIFY_ENABLE == 0`）。注意 secure-off **只跳过 ECC 签名**，**不跳过 body 哈希**——flashboot 在硅片上仍会校验 body SHA-256。所以一个能启动的镜像需要 **0x300 头 + 真实 body SHA-256（secure-off 仍校验 hash，只跳过 ECC 签名）**，并**不需要真实签名密钥**；`hisi-fwpkg image`（route 1）/ `patch-hash`（route 2）填的就是这份真实 hash。要打开 secure boot 的代价与做法见[安全启动与签名](../explanation/05-secure-boot.md)。
