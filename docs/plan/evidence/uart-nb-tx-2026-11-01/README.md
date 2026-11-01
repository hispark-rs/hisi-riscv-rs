# UART 非阻塞发送与共享后端（2026-11-01）

WS63/BS2X 的 `embedded_hal_nb::serial::Write::write` 原先调用阻塞
`write_byte`。FIFO 满时无法返回 `WouldBlock`。HAL `9a8cfaf` 将三芯片
三个实例统一转发到已有的有界 `try_write_byte`，没有引入另一套等待队列。
WS53 错误类型、LSR 错误保留及 FIFO/移位寄存器 flush 语义独立保留；旧芯片
flush 仍只检查 FIFO。普通 blocking/async 入口不在本次行为修改范围内。

| 验证 | 结果与边界 |
| --- | --- |
| HAL 独立子仓 | WS53/WS63/BS21：445/372/220 主机测试、文档测试、主机及目标 Clippy、非法 feature 检查通过；使用明确的远端开发依赖，不是 registry-only 发布验收 |
| 修复前对照 | 同一 WS63 夹具、同一模型、容量 1，旧 HAL `a52e0dd` 仅输出 BEGIN，10 秒未返回；没有释放背压来掩盖阻塞 |
| 实际 Rust 公共 trait | WS63/BS21，各容量 1/16：满返回、重复拒绝、flush 阻塞状态、顺序 drain 和恢复通过；虚拟对端准确收到填充字节和 `a5`，未收到拒绝的 `ee` |
| 独立外部消费者 | 父仓外、全新 Cargo 缓存、精确远端 HAL/runtime + registry PAC；WS63/BS21 完整链接及容量 16 QEMU 通过，没有绝对路径依赖或父仓 patch |
| 模型自身 | 五种机器 × 三端口 × 两容量，共 30 组；回绕、满写诊断、顺序、FIFO/system reset 通过 |
| 相关回归 | 18 组 RX、TCXO、软件中断、11 项旧 qtest 通过；WS53 UART 正常/断路/取消超时各两轮通过 |
| 实板 | 未操作；RAM、Flash、冷启动及物理 UART 时序均未重新验收 |

QEMU `2c4aa61` 新增显式 `tx-fifo-size` 和 QOM `tx-stalled` 测试配置；默认仍立即
输出。暂停保留 FIFO，释放后输出，满写拒绝并诊断；guest reset 清空数据但不改变
host 设置的故障条件。容量不是物理深度，模型没有 TX IRQ、CTS 或独立移位器时序。
因此不能用它宣称旧芯片 async TX 已完成，也不能关闭 `block_on` 的 WFI 竞态。

模型使用全新源码固定构建目录，未改动之前封存的构建目录。原始日志、ELF、模型
receipt、远端消费者及旧失败对照保存在 `report.json` 指定路径；索引记录校验值。
WS53 取消场景首次因 Unix socket 路径过长，尚未开始客体测试；改用更短的新输出路径
后通过，失败记录保留。BS21 夹具首次漏选 runtime `unstable`，修正 manifest 后通过。

## 复现

1. 用父仓固定模型构建脚本生成新的模型和 receipt。
2. 在 `examples/uart-nb-model` 分别选择 `chip-ws63` / `chip-bs21`，按锁文件构建
   `riscv32imfc-unknown-none-elf` release（`-Zbuild-std=core,alloc`）。
3. 执行 `uv run --script scripts/test-uart-nb-qemu.py --qemu <binary> --elf <elf>
   --machine ws63|bs21 --capacity 1|16 --output <new-directory>`。
4. 子仓独立执行 `scripts/check-ws53-integration.sh`；模型仓执行
   `scripts/ws63_uart_tx_test.py`。父仓 CI 包含上述正常场景。

[HAL 草稿 PR #26](https://github.com/hispark-rs/hisi-hal/pull/26) 叠加 #25；
[模型草稿 PR #28](https://github.com/hispark-rs/hisi-riscv-qemu/pull/28) 叠加 #27。
没有合并受保护分支、升级版本或发布 crate。未发布 HAL/runtime 等依赖仍阻塞正式发布。

上一父仓提交 `669f1e203` 的 mdBook 检查已经通过，但完整 CI 仍有：DMA 文档示例
在非 unstable 配置访问私有模块、计划登记遗漏、历史证据链接缺失，以及依赖审计失败
（crypto-bigint 两个版本、ruzstd）。这些未被本批 UART 验收覆盖，另行处理；没有忽略门控。
