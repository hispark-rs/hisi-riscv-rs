# WS53 专用 ITCM 函数消费者

独立 workspace，默认启用运行时 `ws53-ram-functions`。真实 FlashBoot 入口负责
初始化 RAM；main 不依赖调试器 gate。两个不内联的 Rust 函数与显式只读表位于
`[0x28000,0x2f000)`，不借用启动栈。记录区 `WS53_RAM_FUNCTIONS_REPORT` 保存
实际地址、复制比对和运算结果。独占测试在 main 用 PAC 有界解锁并停止继承 WDT，
不代表通用控制核/WDT 资源交接。没有 UART、引脚或 Flash 控制器操作。

在主工程根目录执行：

```sh
uv run --script scripts/test-ws53-ram-functions.py --output target/ram-functions-build
uv run --script scripts/test-ws53-ram-functions-qemu.py \
  --qemu /path/to/qemu-system-riscv32 --model-receipt /path/to/receipt.json \
  --model-source /path/to/reviewed-qemu-overlay \
  --elf target/ram-functions-build/firmware.elf \
  --receipt target/ram-functions-build/report.json --output target/ram-functions-model
```

构建/源字节收据绑定给定 ELF；脚本会核对当前依赖来源。新输出目录必须不存在。
QEMU 测试验证 Flash 源已经加载、ITCM 尚未预填，复制后 main 看到相同字节，
然后用硬件执行断点分别观察根函数和子函数 PC。正常、复制前毒化、复制后表项
损坏及系统复位恢复共四轮。损坏轮在客体复制比对之后改一项常量，由真实子函数
读出后产生错误结果；不写入诊断状态，不以调试器替换正常指令。

`flash-call`、`flash-data`、`overflow` 和无默认 feature 是拒绝测试消费者，
不能作为有效应用。ELF 审核需要 `--emit-relocs`、禁用 relaxation 及符号信息；
Flash 封装检查须显式使用 `scripts/check-ws53-flash-elf.py --ram-functions`，
默认仍拒绝这种布局。参见运行时 `docs/ws53-ram-functions.md` 的不安全条件。

当前仅软件/模型验收；新布局的 RAM 实板、Flash 实板和断电冷启动未验收。
不会将模型通过或候选18的旧布局资格算作本程序硬件通过，也不授予 SFC 擦写资格。
