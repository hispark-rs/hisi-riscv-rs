# 如何打包成可启动镜像（hisi-fwpkg）

裸 ELF/bin 直接烧进 app 分区**不会启动**。flashboot 会**无条件**跳到 `app 分区 + 0x300`（WS63 上 app 分区 = flash `0x230000`，入口 = `0x230300`），所以 app 分区开头必须是 0x300 字节 HiSilicon **镜像头**，后面才是代码。字段布局见[应用镜像格式与签名](../reference/06-image-format.md)，启动流程见[启动流程](../explanation/02-boot-flow.md)。

从 0.6.0-alpha 开始，镜像语义统一收敛到 `hisi-fwpkg plan`：

- `hisi-fwpkg` 负责解释 ELF/headered ELF/raw image/fwpkg，计算 header、body range、hash、base address 和完整 flash image。
- `probe-rs` 不理解 HiSilicon 镜像格式，只用通用裸 bin 下载能力：`probe-rs download --binary-format bin --base-address <plan.base_addr> <plan.image>`。
- `patch-hash` 只保留给需要 ELF 元数据的路径，例如 `probe-rs run` / `embedded-test`；普通 smoke/download 不再直接烧 ELF。

> 安装：`cargo +stable install hisi-fwpkg-cli --version 0.3.2`。

## 生成 plan image

`plan` 会输出 JSON，并可同时写出要烧录的完整 flash image：

```bash
cargo build -p blinky --release

hisi-fwpkg plan \
    target/riscv32imfc-unknown-none-elf/release/blinky \
    --chip ws63 \
    --image-output blinky.img \
    > blinky.plan.json
```

计划文件里最常用的字段：

| 字段 | 含义 |
|------|------|
| `base_addr` | `probe-rs --base-address` 应使用的 flash 地址 |
| `image_len` | 写入 image 的字节数 |
| `body_range` | flashboot 会连续读取并校验 SHA-256 的 body 范围 |
| `code_area_len` | 写入 header 的 body 长度 |
| `code_area_hash` | 写入 header 的 body SHA-256 |
| `write_chunks` | 下载器应写入的 flash chunk；当前 smoke 路径写一个完整 image chunk |
| `source_fwpkg` | 输入为 `.fwpkg` 时存在；记录包内分区表、CRC 状态、burn 地址/大小，和 `hisiflash info --json` 使用同一套 parser 语义 |

烧录时从 plan 读取基址：

```bash
BASE_ADDR=$(uv run scripts/read-flash-plan-base.py blinky.plan.json)

probe-rs download --chip WS63 \
    --chip-description-path HiSilicon_WS63.yaml \
    --binary-format bin \
    --base-address "$BASE_ADDR" \
    blinky.img
```

## WS63：link-time header 仍然可复用

WS63 仍使用 `hisi-riscv-rt` 的 `boot-header` feature，把 0x300 header 在链接期放进 ELF。区别是：smoke/download 路径不再让 probe-rs 解释 ELF 段，而是让 `hisi-fwpkg plan` 按 flashboot 实际模型展开：

- body 按 `PT_LOAD.p_paddr` 排列；
- 段间 gap 填 `0xFF`，匹配擦除态 flash；
- hash 覆盖 flashboot 实际连续读取的 body；
- 输出 header + body 的完整 image，避免旧 flash 残留进入校验范围。

如果你需要 `probe-rs run <elf>` 或 embedded-test 的半主机测试，才使用：

```bash
hisi-fwpkg patch-hash target/riscv32imfc-unknown-none-elf/release/blinky
probe-rs run --chip WS63 --chip-description-path HiSilicon_WS63.yaml \
    target/riscv32imfc-unknown-none-elf/release/blinky
```

这条路径保留是因为 `run`/embedded-test 需要 ELF 里的符号、测试元数据和 semihosting 信息；它不是普通下载路径的格式事实源。

## BS2X：同样走 plan image

BS2X 暂无链接期 boot-header，`hisi-fwpkg plan` 会从 ELF/body 构造 0x300 header + body image：

```bash
hisi-fwpkg plan target/riscv32imfc-unknown-none-elf/release/blinky \
    --chip bs21 \
    --image-output blinky.img \
    > blinky.plan.json
```

BS2X 默认 app 基址来自 `hisi-fwpkg` 的芯片默认值；真机烧录前仍应对照板子的分区表确认。

## 产 `.fwpkg`（hisiflash 路径）

厂商串口/YMODEM 路径继续使用 fwpkg 容器：

```bash
hisi-fwpkg pack -o blinky.fwpkg --chip ws63 \
    target/riscv32imfc-unknown-none-elf/release/blinky
```

`pack` 选项：

| 参数 | 默认 | 说明 |
|------|------|------|
| `<input>` | - | 输入 ELF 或裸 `.bin` |
| `-o, --output <PATH>` | - | 输出 `.fwpkg` |
| `-c, --chip <ws63\|bs21>` | `ws63` | 目标芯片，决定默认 app 分区地址 |
| `--app-addr <ADDR>` | 芯片默认 | 覆盖 app 分区地址，如 `0x230000` |
| `--name <NAME>` | `app` | fwpkg 内分区名 |

## 用脚本一把梭

```bash
CHIP=ws63 hil/pack.sh blinky       # -> blinky.img + blinky.plan.json
PROBE_RS_YAML=/path/HiSilicon_WS63.yaml hil/flash.sh blinky
FWPKG=1 hil/pack.sh blinky         # 额外产出 blinky.fwpkg
```

`hil/pack.sh` / `hil/flash.sh` 只是 runner wrapper，镜像规则仍由 `hisi-fwpkg plan` 决定。

## 关于签名

开发芯片 secure boot 是关的（efuse `SEC_VERIFY_ENABLE == 0`）。secure-off **只跳过 ECC 签名**，**不跳过 body hash**。所以一个能启动的开发镜像需要 **0x300 header + 真实 body SHA-256**，不需要真实签名密钥。要打开 secure boot 的代价与做法见[安全启动与签名](../explanation/05-secure-boot.md)。
