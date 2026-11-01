# UART 共用异步操作生命周期

HAL `d7e8ea5`（[草稿 PR #28](https://github.com/hispark-rs/hisi-hal/pull/28)，叠加 #27）
把 WS53/WS63 RX 和 WS53 TX/flush 接入同一个 `uart/async_wait.rs`。删除独立
`poll_tx`/`TxWait`；共用代码负责 Waker 登记、屏蔽、首轮尝试、开启后复查和 Drop 清理。
后端仅提供有界完成检查与通知控制，保留各自寄存器、IRQ 确认、数据及所有权契约。

本批没有改变公开签名、移交前提或声明新的芯片能力。WS53 写入仍每次接受一个字节，
flush 仍检查 FIFO 和移位器；Pending 写入不能发送数据，取消 flush 不丢弃在途字节。
WS63 async TX 仍是阻塞实现，不能把 RX/TX 共用生命周期解释成已修复旧 TX。
未轮询的 public future 不执行 async body，不构造 guard；内部 guard 的未轮询 Drop
也不触碰硬件。已轮询 guard 的停止/清理顺序保持原有行为。

验证结果与来源校验值见 [report.json](report.json)：

- HAL 独立副本 WS53/WS63 实验/BS21 主机测试 446/377/220 项，两个 WS63 稳定
  配置各 335 项；文档、host/target Clippy 和错误 feature 门控通过。
- 共用 13 项主机测试由子仓维护，原 TX 测试迁入共用模块，新增实际 guard 的
  取消写入不发数据、取消 flush 保留已接受数据和释放 Waker 检查。
- WS63 真实 Rust RX 在模型容量 1/16 通过；WS53 TX 257 字节、三种取消、flush、
  RX 保留和真实 IRQ 断言通过。WS53 RX 正常/阻断/取消恢复共 8 轮通过。
- 父仓外消费者从远端精确提交解析 HAL/RTOS，四种 profile 的 host 与 RISC-V
  检查通过，并编译验证原 UART async trait 入口。使用已有下载缓存，固定 Git
  开发覆盖不是 registry 发布验收。消费者输入和锁文件保存在本目录。

独立回归首次遇到 `/tmp` 配额不足，改用工作区中新建的临时/构建目录后通过。
没有删除旧产物、证据、波形或恢复备份。模型初次 RX socket 路径过长、TX 结束标记
缺 CR 的运行器失败也保留，修正工具后重跑，没有降低整段串口输出或寄存器断言。

原 CI 的 WS63 RX 命令误带第二个脚本名作位置参数，已复现 argparse exit 2 并修正。
新增独立 TX 运行器只依赖已跟踪的通用 QEMU/ELF 工具，不引入旧未跟踪的 host 脚本链。
CI 已接入 TX 检查和报告上传；远端本批 workflow 结果仍需另行确认。

复现 TX：按现有锁文件构建 `examples/ws53/Cargo.toml` 的 `ws53_ram_smoke`，启用
`uart2-async-tx-smoke`，然后执行 `uv run --script scripts/test-ws53-uart-async-tx-qemu.py
--qemu <固定模型> --elf <ELF> --output <新目录>`。完整命令也在 CI 中，模型仍固定
原 `2c4aa61`，没有为此次合并修改模型语义。

本轮没有操作实板或发布 crate。物理时序、Flash/冷启动、WFI 竞态、旧 TX 阻塞及
未发布依赖继续保持未验收，不自动继承历史硬件结论。
