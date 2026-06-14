# 点亮第一个 LED（blinky）

本课我们把 `blinky` 示例跑起来：先在 QEMU 里启动它，再烧到真正的 WS63 开发板，
亲眼看到板载 LED 以 1 秒周期闪烁（亮 0.5 秒、灭 0.5 秒）。

> 这条路径已在真实芯片上验证通过（2026-06-14，GPIO0 翻转正常）。照做即可成功。

blinky 把 **GPIO0** 配成推挽输出，然后在死循环里拉高、延时、拉低、延时。
开发板上 GPIO0 一般接着板载 LED。

## 第 1 步：编译 blinky

在仓库根目录执行：

```bash
cargo build -p blinky --release
```

产物 ELF 在：

```
target/riscv32imfc-unknown-none-elf/release/blinky
```

> 关于构建目标和产物路径的细节，见 [构建一个示例](../how-to/build-example.md)。

## 第 2 步：先在 QEMU 里跑

烧真机之前，先用 QEMU 确认程序能正常启动。用 WS63 机器模型加载这个 ELF：

```bash
qemu-system-riscv32 -M ws63 -nographic -bios none \
    -kernel target/riscv32imfc-unknown-none-elf/release/blinky
```

QEMU 会加载并运行 blinky。blinky 只翻转 GPIO0、没有串口输出，
所以控制台**不会**打印任何东西——这是预期的：程序正在安静地循环翻转引脚。

按 `Ctrl-A` 然后按 `X` 退出 QEMU。

> 想"看见" GPIO 在 QEMU 里翻转，可以在第 4 课用 `gpio_irq` 这类带串口输出的示例。

## 第 3 步：打包成可启动镜像

真机的 flashboot 期望一个带 `0x300` 启动头的应用镜像。用 `hisi-fwpkg` 打包：

```bash
hisi-fwpkg image -o blinky.img \
    target/riscv32imfc-unknown-none-elf/release/blinky
```

这会生成 `blinky.img`。安全启动是关闭的，所以 hisi-fwpkg 用一个全零的占位签名，
无需真正签名。镜像格式细节见 [应用镜像格式与签名](../reference/image-format.md)。

## 第 4 步：烧录到开发板

插上开发板（确保用的是打过补丁的 probe-rs 分支），把镜像下载到应用分区
（flash 地址 `0x00230000`）：

```bash
probe-rs download --chip WS63 \
    --chip-description-path HiSilicon_WS63.yaml \
    --binary-format bin --base-address 0x00230000 \
    blinky.img
```

> `HiSilicon_WS63.yaml` 来自打补丁的 probe-rs 分支仓库；把它放在当前目录或写全路径。
> 上游 probe-rs 没有 WS63 支持，必须用这个分支——详见
> [用 probe-rs 烧录到真机](../how-to/flash-probe-rs.md)。

下载成功后复位芯片，让 flashboot 跳进应用（应用入口在 `app + 0x300`）：

```bash
probe-rs reset
```

## 第 5 步：看 LED 闪烁

复位之后，看你的开发板：**板载 LED（GPIO0）开始闪烁**，亮 0.5 秒、灭 0.5 秒，
不断循环。

成功了！你刚刚用 Rust 让一块 WS63 芯片亮起了第一个 LED。

> 觉得"编译 → 打包 → 烧录 → 复位"四步太繁琐？仓库提供了一个 cargo runner
> （`hil/cargo-run-hw.sh`），能让 `cargo run -p blinky --release` 一条命令搞定全部。
> 用法见 [用硬件 runner 让 cargo run 烧真机](../how-to/hardware-runner.md)。

下一课我们让芯片"开口说话"——在 QEMU 里跑出串口打印 ——
[第一个 UART 程序（uart_hello）](03-uart-hello.md)。
