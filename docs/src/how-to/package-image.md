# 如何打包成可启动镜像（hisi-fwpkg）

裸 ELF/bin 烧进 app 分区**不会启动**。flashboot 会**无条件**跳到 `app 分区 + 0x300`（WS63 上 app 分区 = flash `0x230000`，故入口 = `0x230300`）。所以 app 分区开头必须放一段 0x300 字节的 HiSilicon **镜像头**，后面才是你的代码。`hisi-fwpkg` 负责补这层。镜像头的字段布局见[应用镜像格式与签名](../reference/image-format.md)，启动流程见[启动流程](../explanation/boot-flow.md)。

> 安装：`cargo install --git https://github.com/hispark-rs/hisi-fwpkg`（或 `cargo install hisi-fwpkg-cli`）。

## 两个子命令：`image` vs `pack`

`hisi-fwpkg` 自动从 magic 识别输入是 ELF 还是裸 bin，两个子命令各产一种产物：

| 子命令 | 产物 | 内容 | 谁用 |
| --- | --- | --- | --- |
| `image` | `*.img` | 0x300 HiSilicon 头 ‖ body（含 body 的 SHA-256） | **probe-rs download 路径**（验证主路径） |
| `pack` | `*.fwpkg` | 把上面的 image 再包进单分区 fwpkg（V1 容器 + CRC） | 厂商 hisiflash / YMODEM 路径 |

### 产 `*.img`（probe-rs 路径用）

```bash
hisi-fwpkg image -o blinky.img \
    target/riscv32imfc-unknown-none-elf/release/blinky
```

`image` 只有 `-o/--output <OUTPUT>` 和一个位置参数 `<INPUT>`（ELF 或裸 bin）。app 基址在烧录时由 `probe-rs --base-address` 给（见[如何用 probe-rs 烧录](flash-probe-rs.md)），所以 `image` 自身不需要芯片/地址参数。

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

`hil/pack.sh` 封装了上面两步，按示例名解析 ELF：

```bash
CHIP=ws63 hil/pack.sh blinky       # -> examples/ws63/target/.../blinky.img（默认只产 .img）
FWPKG=1   hil/pack.sh blinky       # 额外再产一个 blinky.fwpkg
```

`CHIP` 决定 app 基址（`APP_ADDR=` 可覆盖），脚本跑完会把两条烧录命令（probe-rs / hisiflash）打印出来供复制。

## 关于签名：本片不需要真签名

镜像头里有签名字段，但**开发芯片 secure boot 是关的**（efuse `SEC_VERIFY_ENABLE == 0`），所以 `hisi-fwpkg` 产的 **dummy 全零签名 + 正确的头**就足够启动，**不需要真实签名密钥**。要打开 secure boot 的代价与做法见[安全启动与签名](../explanation/secure-boot.md)。
