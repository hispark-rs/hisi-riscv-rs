# WS53 SPI0 RAM 与逐位模型验证

**状态：** 配套证据记录；最新验收状态以 [实施记录](ws53-adaptation.md) 为准。

状态：实验。SPI0 的受控 RAM 基础传输已通过实板与同 ELF 模型对照，本场景的
Flash 启动尚未通过，真正冷启动和正常外部 SPI 器件未测。QSPI1 本阶段只有模型证据。
完整适配进度见[实施记录](ws53-adaptation.md)。

后续Flash首轮已实测但失败：SPI23虽然完成真实烧录及4MiB回读，却停在GPIO1访问
阶段，未完成传输；不能计为Flash通过。已精确恢复原厂镜像和当前NV，原厂AT与
SWD正常。SDK要求先开启AGPIO时钟；隔离RDL/PAC描述已准备，驱动和模型仍待接入
及受控实测。见[首轮失败档案](evidence/ws53-spi23-first-flash-2026-10-28/README.md)。

## 接线与来源

依据 SDK `163d9150ab94a02d09a05f623351f9facee3025e` 的
`src/drivers/chips/ws53/porting/pinctrl/pinctrl_porting.c`、SPI port 和 v151_100
寄存器头文件，使用应用核 SPI0 `0x52087000`、IRQ65。没有套用控制核 SPI 基址。

| 信号 | HAL pad / SDK pin | mode | 当前 A 板 DSLogic |
|---|---|---|---|
| 时钟 SCLK | MGPIO8 / SDK8 | 2 | D15 |
| 主出 MOSI | MGPIO9 / SDK9 | 2 | D14 |
| 主入 MISO | MGPIO12 / SDK12 | 1 | D10 |
| 软件 CS | AGPIO4 / SDK36，GPIO1.bit4 | 0 | D1 |
| 测试标记 | MGPIO17 / SDK17 | 0 | D5 |

当前台架仅有分析仪输入，VDDIO 已接板上 3V3，共地。程序先释放 SPI0 的其他已知
复用候选，保持 native CS 未路由；全程不改 SFC/QSPI 或 SWD 引脚。MISO 由内部
上下拉提供已知 0/1，不是外部器件响应或正常全双工链路的电气验收。

首次选用 AGPIO3/D0 时，前置检查发现该脚已经是输出（GPIO1 OEN=`0xff7`），
在 SPI 发送前拒绝，随后物理复位。原脚本在验收异常后漏存子报告，原日志保留，
另用独立 AT 查询补证原厂恢复；已修复新 SPI 路径的失败报告保存。改选初始为输入
的 AGPIO4，保留同一检查，没有强行接管原输出。

SDK 中 `spi_porting_test_spi_init_pin()` 使用 MGPIO6/7 的旧测试配置，而本产品的
pinctrl 表声明它们不存在；实际测试依据完整复用表，不照搬该片段。

## 已验收的 RAM 子集

一轮包含四种模式 × 100/500kHz × 8/32 位的 16 个组合，96 帧、1536 位。
发送非平凡数据，包括 00、FF、96、3C 和完整 32 位模式；接收端分别验证全零和
全一。三轮正式实板共 288 帧、4608 位，AP0、串口、DSLogic 和同 ELF 模型一致。

每阶段用 T0（GPIO 置高前）、T1（置高后）、T2（置低前）、T3（置低后）独立约束
标记窗口。物理解码要求完整片选、全部时钟边沿、正确 CPOL/CPHA、逐字数据及
稳定 MISO。4MHz 采样、1.65V 阈值；100kHz 请求均值为 100kHz，500kHz 阶段均值
约 500000–500505Hz，保留一个采样点的量化差异。

对照同时检查逐周期和每字首尾跨度。每字只给两个采样端点的 500ns 预算，不能
把预算逐周期累加来放过持续频率偏差。这不等于校准了分析仪时钟或证明模拟时序。
SPI0 基线 `SPI_CLOCK=0`、`M_CLKEN1.bit11=0`，由现有 HAL 构造器开启已确认的门；
源仍来自接管前配置，不代表通用时钟树初始化。

所有成功轮均验证控制器禁用、IRQ/DMA 关闭、pad/方向与共享时钟字恢复，之后以
整板复位恢复原厂 AT，不恢复已被 RAM 覆盖的旧 CPU 上下文。五次 SWD 健康检查
共 3000 次 DPIDR 零错，暂停/单步/继续正常。没有 Flash 擦写或永久配置变更。

## 模型和错误路径

隔离工作树提交 `7436a3f6c695f6d73ddd500f9198da9dfde7deb3`，构建策略固定 55 个
源、补丁和构建脚本，索引 SHA 为
`ae9013136c3486546389f2d91f2c7eec30db7e8637d9c8762bf6e6c5bc6e175e`。
目前只在本地，远端 CI 没有接入这个增量。

逐位模型须显式启用 `ws53-spi.assume-profile=on`、
`ws53-spi.assume-wire-profile=on` 和 `ws53-pinctrl.assume-spi-wire=on`。
原有 FIFO/XOR 对端模型默认行为保留；逐位模式实际从所选 MISO pad 采样，
不会用 XOR 计算结果替代输入。原有基础 profile 日志仍包含默认 XOR 对端说明，
是否采用逐位输入以额外的 wire profile、来源和 trace 为准。

模型覆盖 SPI0 的 12 组 SDK 路由组合及 QSPI1 的 MGPIO17/18/19 主路由；QSPI 的
其他复用路径不在本增量内。原生 CS 路由、多 MISO、缺输入或悬空输入使传输停住；
非零采样/驱动延迟明确诊断为未实现，不用返回零掩盖。

208 帧独立 MISO 对端覆盖两实例、8/32 位、四种模式、两档分频，并在错误边沿
给出反值，验证控制器确实在指定边沿采样。22 组路由缺失、重复输入、暂停、取消和
复位恢复通过；暂停注入不证明物理门控时 pad 的保持行为。105 项原始记录和采样
损坏拒绝测试通过。旧 SPI 41 项非法访问、I2C 38 项、SPI DMA 32 组合/26 拒绝、
I2C 数字路由 13 组和 WS63 原 11 项 qtest 回归通过。

后续修复（2026-10-28）：RDL `9975fc2`、PAC `ccb0c00` 补上 SPI0 bit0、QSPI1
bits5/6 的选择性读清描述。生成的非文档 Rust 代码和布局不变。新模型 `f8ff081`
分离碰撞状态与 IRQ，并纠正把主机 FIFO 溢出映射为从机 TXE 的错误。16 项状态/
IRQ 顺序测试通过，但保持 IRQ 至显式确认仍是保守模型策略，硬件联动尚未证实。
首轮测试遗漏释放外部碰撞注入线，失败原样保留，修正夹具后重新构建并回归。

电脑重启报告后重新检查设备并重做 SPI0 RAM 波形，新来源 ELF 与此前一致，
96 帧1536位与新模型通过同一严格比较。原厂 AT、1800 次 SWD 读取及CPU调试正常。
本次状态语义、回归及恢复证据见[184项补充档案](evidence/ws53-spi-status-2026-10-28/README.md)。
错误位在正常实板采集中为零，不能把这轮正常传输当作真实碰撞/从机错误验收。

## 复现入口

普通 Cargo 构建不需要 Python 或 SDK：

```bash
cargo build --release -Zbuild-std=core,alloc \
  --manifest-path examples/ws53/ram_smoke/Cargo.toml --features spi-wire-probe
```

证据构建与同 ELF 模型测试：

```bash
uv run scripts/ws53_spi_wire_build.py --output target/my-spi-ram
uv run scripts/test-ws53-spi-wire-qemu.py \
  --qemu target/my-spi-model/qemu/build/qemu-system-riscv32 \
  --model-receipt target/my-spi-model/receipt.json \
  --model-source ../hisi-riscv-qemu-ws53-spi-wire \
  --elf target/my-spi-ram/firmware.elf --receipt target/my-spi-ram/receipt.json \
  --output target/my-spi-model-test
```

模型由 `scripts/ws53_ci_qemu_spi_wire_build.py` 构建，需上述本地提交、QEMU 依赖及
Python 3.12；用 `--overlay-root`、`--output`、`--python` 指定环境，可通过
`--upstream-mirror` 使用已有上游镜像。普通用户应用不在 Cargo 构建中生成模型。

新的状态修复模型使用 `scripts/ws53_ci_qemu_spi_status_build.py`，固定 `f8ff081`。
执行 `scripts/test-ws53-spi-status-qemu.py`，参数同上，并指定新构建的模型收据，
可一并运行状态顺序、FIFO/DMA、208帧独立输入和同ELF逐位回归。旧策略仍固定旧
模型，不能把新二进制冒充成旧收据；专项包装器显式选择新来源准入。

当前 A 板专用 `hil/ws53-spi-wire-capture.py` 需要新鲜健康报告及匹配的 ELF/receipt，
会加载 RAM、采集十秒并在最后复位恢复。用 `scripts/analyze-ws53-spi-wire.py` 解码，
再用 `scripts/compare-ws53-spi-wire.py` 重新核对模型和物理原始数据。每次使用新
输出目录；输入或设备身份不符时停止，禁止全引脚自动扫描。

## 证据和未关闭项

[208 项封存证据](evidence/ws53-spi-wire-2026-10-28/README.md)保存来源、模型、
三轮正式实板、错误路径、回归、拒绝与早期发现记录。原始采样和厂商内容私有保留。

Flash 自主启动与真实断电冷启动、本 SPI 场景的 hisiflash 安装和恢复、QSPI 实板、
正常外部 SPI 器件、原生 CS 连续性、16 位/打包/双四线、高频电气、SPI DMA/IRQ/async
实板和生产跨核资源交接仍待实施或验收。所有 API 继续保持实验门控。
