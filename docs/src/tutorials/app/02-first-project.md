# 从模板创建你的第一个工程

这是应用开发者路径**最关键的一课**。我们用 `cargo generate` 从模板生成一个
**你自己的** blinky 工程，先在 QEMU 里跑通，再烧到真正的 WS63 开发板，
亲眼看到板载 LED 闪烁。

> 这条 blinky 路径已在真实芯片上验证通过（2026-06-14，GPIO0 翻转正常）。照做即可成功。

## 第 1 步：从模板生成工程

用 `cargo generate` 拉取模板并回答几个提示：

```bash
cargo generate --git https://github.com/hispark-rs/hisi-rs-template
```

按提示回答（默认值就是我们要的）：

- **项目名**（`Project Name`）：随便起一个，比如 `my-blinky`。
- **Target chip**：选 `ws63`（默认）。
- **Starter app**：选 `blinky`（默认）。
- **App partition flash address**：保持默认 `0x00230000`（WS63 的应用分区地址，已验证）。

生成完成后进入工程目录：

```bash
cd my-blinky
```

这个工程是**自包含的**：它的依赖（`hisi-riscv-hal` / `hisi-riscv-rt` / `ws63-pac`）
都来自 crates.io，它自带 `rust-toolchain.toml`（钉死 `hisi-riscv` 工具链）、
`.cargo/config.toml`（设好目标和 QEMU runner）和一个 `justfile`。
你**不需要**克隆任何monorepo。

## 第 2 步：在 QEMU 里跑

先用 QEMU 确认程序能正常启动：

```bash
just run
```

这会 `cargo build --release` 再用 `-M ws63` 启动 QEMU。blinky 只翻转 GPIO0、
没有串口输出，所以控制台**不会**打印东西——这是预期的：程序正在安静地循环翻转引脚
（机器 trace 里能看到 GPIO0 每 500 ms 变一次）。

按 `Ctrl-A` 然后按 `X` 退出 QEMU。

## 第 3 步：烧到真正的开发板

插上 WS63 开发板，一条命令完成"编译 → 打包 → 下载 → 复位"：

```bash
just flash
```

`just flash` 依次做了这些事：

1. `cargo build --release` 编出 ELF；
2. `hisi-fwpkg image` 给 ELF 加上 `0x300` HiSilicon 启动头，打包成可启动镜像；
3. 用打补丁的 probe-rs 分支把镜像 `download` 到应用分区 `0x00230000`；
4. `probe-rs reset` 复位，flashboot 跳进应用入口（`app + 0x300`）。

> **不需要真正签名**：开发芯片的安全启动是关闭的（efuse `SEC_VERIFY_ENABLE == 0`），
> 所以 `hisi-fwpkg` 写入一个全零的"占位签名"启动头就足够了。镜像格式细节见
> [应用镜像格式与签名](../../reference/image-format.md)。
>
> 如果 `HiSilicon_WS63.yaml` 不在当前目录，指给它：
> `just CHIP_DESC=/path/to/HiSilicon_WS63.yaml flash`。
> probe-rs 分支与 YAML 的细节见 [用 probe-rs 烧录到真机](../../how-to/flash-probe-rs.md)。

想在烧录的同时顺便看 UART0 的启动日志，可以用：

```bash
just run-hw PORT=/dev/ttyUSB0
```

它会先 `flash`，再把 UART0（CH340 串口，`/dev/ttyUSB0` @ 115200 8N1）接到终端。

## 第 4 步：看 LED 闪烁

复位之后，看你的开发板：**板载 LED（GPIO0）开始闪烁**，亮 0.5 秒、灭 0.5 秒，
不断循环。

成功了！你刚刚用一个**完全属于你自己**的 Rust 工程，让一块真正的 WS63 芯片
亮起了第一个 LED。

下一课我们把它改造成一个会"说话"的 UART 程序 ——
[改造成一个 UART 程序](03-uart.md)。
