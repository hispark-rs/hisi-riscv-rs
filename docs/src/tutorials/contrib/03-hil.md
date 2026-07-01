# 第一次硬件在环测试

"硬件在环"（HIL，Hardware-In-the-Loop）就是：把程序烧进真芯片、让它真的跑起来，
再从外部观察它的行为，确认真实硬件上一切正常。本课我们用 `blinky`——
那个已经在真实芯片上验证通过的示例——完成你的第一次 HIL 测试，保证成功。

> 我们特意选 blinky：它的 GPIO0 翻转是当前**确认可观测**的真机行为
> （上一课提过，真机串口 banner 还在打磨中）。

## 你需要准备

- 一块 WS63 开发板，已连上电脑。
- 第 1 课装好的 `hisi-fwpkg` 和打过补丁的 probe-rs 分支。
- 一根 USB 串口线连到板子的 **UART0**，在系统里通常是 `/dev/ttyUSB0`（CH340 适配器），波特率 `115200 8N1`。

> 注意：`/dev/ttyACM0` 是 J-Link 的 VCOM，**不是**应用 UART，别接错了。

## 第 1 步：把 blinky 烧进真板

从仓库根目录走"编译 → 补哈希 → 下载 → 复位"四步：

```bash
cargo build -p blinky --release

# WS63 用 hisi-riscv-rt 的 boot-header 特性：0x300 启动头在链接期就已烘进 ELF，
# 链接后只需补上 body 的 SHA-256（在原文件就地填写），无中间 .img、无 image 步骤。
hisi-fwpkg patch-hash \
    target/riscv32imfc-unknown-none-elf/release/blinky

probe-rs download --chip WS63 \
    --chip-description-path HiSilicon_WS63.yaml \
    target/riscv32imfc-unknown-none-elf/release/blinky

probe-rs reset
```

> `hisi-fwpkg patch-hash` 把 ELF 里 `0x300` 启动头之后的 body SHA-256 就地补全；
> 头已在链接期烘入，flashboot 复位后跳进 `app + 0x300`。安全启动关闭时，flashboot
> **仍校验 body hash**，只跳过 ECC 签名（efuse `SEC_VERIFY_ENABLE==0`）——所以镜像需要
> 一份**真实** body SHA-256，没有"占位签名"能让它启动。
> `HiSilicon_WS63.yaml` 来自打补丁的 probe-rs 分支仓库——上游 probe-rs 没有 WS63 支持。
> （BS2X 还没有链接期 boot-header，仍走 `hisi-fwpkg image -o app.img` 的 route 1 路线。）
> 细节见 [用 probe-rs 烧录到真机](../../how-to/04-flash-probe-rs.md) 与
> [应用镜像格式与签名](../../reference/06-image-format.md)。

## 第 2 步：观察 GPIO 翻转

复位后看开发板：**板载 LED（GPIO0）开始闪烁**，亮 0.5 秒、灭 0.5 秒。

这就是一次成功的硬件在环观测——程序确实在真芯片上运行，并按预期驱动了真实引脚。
如果手边有逻辑分析仪或万用表，也可以直接量 GPIO0 引脚看到方波。

## 第 3 步：在串口上看启动日志

打开一个串口监视器，盯住 UART0，就能在烧录/复位时看到 flashboot 的启动日志，
确认芯片确实重启并跳进了你的应用：

```bash
stty -F /dev/ttyUSB0 115200 raw -echo
cat /dev/ttyUSB0
```

再做一次 `probe-rs reset`，监视器里就会滚出 flashboot 的启动信息。
看完按 `Ctrl-C` 退出 `cat`。

> blinky 自身不打印串口（它只翻转 GPIO），所以这里看到的是 **flashboot 的启动日志**，
> 不是应用输出。UART0 接线与端口的细节见 [HIL 标记串与环境变量](../../reference/07-hil-markers.md)。

## 第 4 步：认识 HIL 冒烟测试

手动烧一个示例、肉眼看一个结果，是理解 HIL 的好起点。但当示例越来越多时，
我们希望**自动**把每个示例烧上去、读 UART、断言它打印了预期的标记串。
仓库里的 `hil/hil-smoke.sh` 就是干这个的——它是 QEMU 冒烟测试在真硅片上的对应物。

它大致这样工作（概念示意，**本课不要求你真的运行**）：

```bash
PORT=/dev/ttyUSB0 hil/hil-smoke.sh
```

脚本会逐个示例：用 `hil/flash.sh` 烧录 → 读几秒 UART → 用 `grep` 检查预期标记串，
比如 `uart_hello` 应出现 `Hello from WS63`、`timer_irq` 应出现 `timer irq #`。
而 blinky 因为没有串口输出，脚本会特别提示"请用 LED / 逻辑分析仪人工确认"——
正是你在第 2、3 步亲手做的事。

> HIL 框架的整体设计、标记串约定、它和 QEMU 冒烟测试的关系，见
> [HIL 测试框架](../../explanation/07-hil-framework.md) 与
> [运行 HIL 冒烟测试](../../how-to/07-run-hil-tests.md)。

恭喜！你已经完成了贡献者路径的全部三课：克隆仓库、装好工具链，在 QEMU 里跑通了
示例集，最后在真正的 WS63 芯片上完成了第一次硬件在环测试。

接下来想做点什么？

- 想完成具体任务（新建工程、加驱动、调试读内存）——看 [操作指南](../../how-to/00-index.md)。
- 想查命令、地址、API——看 [参考](../../reference/00-index.md)。
- 想搞懂背后的原理——看 [原理与背景](../../explanation/00-index.md)。
