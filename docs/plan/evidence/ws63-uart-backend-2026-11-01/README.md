# WS63 UART 后端纠错与共用等待验证

本批延续按 IP/行为收敛 HAL；Embassy 仍归 `hisi-rtos`，不增加执行器或时间队列。
HAL 的共用 `ReceiveWait` 负责轮询、Waker 和取消，两芯片的寄存器/错误/所有权留在后端。

此前 `3ff019d` 的主机测试不能证明真实寄存器访问正确。实际 Rust 夹具发现两项问题，
分别提交：`7eda6f8` 复用 blocking 的 LINE_STATUS/data 路径，移除未经核实的计数别名；
`1a456ab` 在 pending 接收时选择已有一字节 FIFO 门限，不复位 FIFO。原构造器的
1/4 DMA 门限会让三字节输入无法触发容量 16 模型的 IRQ。取消保留数据，门限不回滚；
后续 DMA 设置自己的门限。实验接口签名不变，行为变化没有继承实板验收。

QEMU `7c88dac` 合并既有 WS53 模型 `27def194`，并保留 TCXO 修正。RX enable/status
位号依 WS63 v151 SDK/PAC 核对；接收、消费、屏蔽、重新使能和门限改变都重算 IRQ。
默认合成容量 1，容量 16 用于检查合并通知和背压，不宣称物理 FIFO 深度。0x4c 别名
仍明确报 guest-error，未为使旧驱动通过而添加假计数。模型完整限制见子仓 UART 文档。

## 结果与反例

- 最终 WS63 HAL 366 项主机测试和目标 Clippy 通过；三芯片远端精确提交的外部消费者
  在全新 Cargo 缓存下通过主机/目标检查。显式 runtime Git override 是开发图，非发布验收。
- QEMU UART 六个机器/容量配置各三路通过；11 项原 qtest、两芯片 TCXO 通过。
- 实际 Rust async Read 的 IRQ54、三字节保留、空通知/空读、取消和再次启用，在容量
  1/16 均通过。夹具直接 poll Future，以隔离尚未修复的 `block_on` 休眠问题。
- 旧 `3ff019d` 在容量 1 报未核实计数别名，poll 仍 Pending；只修状态读取的中间版本
  在容量 16 等不到 IRQ。原始失败完整保留，不改写为成功。
- WS53 UART 三场景的正常及失败后恢复另见本目录 JSON；真实波形、RAM、Flash 复位
  和断电启动均未复测。本轮没有访问实板。

精简来源与状态在 [report.json](report.json)。大型构建和原始日志保存在
`validation/ws63-uart-2026-11-01/`，报告记录路径与校验值；无厂商源码/备份上传。
SDK 来源是本机快照，无 Git 元数据，因此记录文件哈希而不编造 SDK 提交。
合并后沿固定构建器重新构建，未复用旧二进制。一次复用旧 build 目录的尝试被 GCC16
对上游 const 的 Werror 阻止，原日志保留；干净构建遵循仓库既有 disable-werror 配置。

## 复现

```sh
cargo build --locked -Zbuild-std=core,alloc --manifest-path examples/ws63-uart-async-model/Cargo.toml \
  --release --target riscv32imfc-unknown-none-elf
uv run --script scripts/test-ws63-uart-async-qemu.py --qemu <模型二进制> \
  --elf examples/ws63-uart-async-model/target/riscv32imfc-unknown-none-elf/release/ws63_uart_async_model \
  --capacity 16 --output <新目录>
```

另用 `--capacity 1`；模型构建、登记测试与报告上传已接入父仓模型 CI。
本批之前 `77404271e` 的远端两次运行均在 SPI DMA 场景超时，BS20 构建通过；
不能把本地 UART 通过写成整条 CI 通过。该 SPI DMA 失败与新的远端运行待独立验收。

未关闭：TX 阻塞式 async、线路错误/溢出、物理时序/深度、真实 IRQ 竞态、DMA 排空、
WFI 交接及发布依赖。当前库消费者检查不替代 registry-only 或实板验收。
