# UART 模型回归的可复现入口

本项关闭 UART 整改中“运行器依赖未跟踪历史脚本”的缺口，不扩大硬件验收范围。
当前 UART 接收实现和软件验收见 [UART 生命周期证据](evidence/uart-async-lifecycle-2026-11-01/README.md)。
远端干净检出、新依赖缓存和独立模型构建的实际结果见
[本轮验收](evidence/uart-model-repro-2026-11-01/README.md)；八轮场景通过，未操作实板。

## 工具职责

- `ws53_uart_rx_build.py` 用已跟踪的 `ws53_ci_elf_build.py` 收集依赖、配置、工具链和
  标准库来源，使用 `check-ws53-flash-elf.py` 检查内存布局。v2 receipt 记录构建及
  全部直接辅助工具哈希，旧 v1 不自动升级。
- `elf_panic_sites.py` 根据完整 DWARF 编译路径、行号和可执行地址绑定具名失败点，
  同名同内容但其他路径的源码不能借用该 ELF 的编译证据。
- `qemu_test_support.py` 只管理 QMP/qtest、进程清理和具名 ELF 对象观察；没有串口设备、
  J-Link 或烧录接口。初始化失败释放已打开的连接，协议错误不能成为零值观测。
- `test-ws53-flash-uart-rx-qemu.py` 只持有 UART 场景策略。它不再导入温度、PWM、RTC
  等旧验收脚本。保留实际 IRQ42、RX 数字 pad 对端、取消、独立超时及故障后恢复断言。

HAL 无变化；测试维护在父仓，因为它验证运行时、HAL、ELF 和模型的组合。

## 运行

先按 `.github/workflows/ws53-model-contracts.yml` 从远端取得固定 overlay 及子模块、
获取锁定 Cargo 和 uv 依赖并独立构建 QEMU。然后执行：

```sh
uv run scripts/test-qemu-test-support.py
uv run scripts/ws53_uart_rx_build.py --output /tmp/ws53-uart-build
uv run scripts/test-ws53-uart-receipt.py --receipt /tmp/ws53-uart-build/receipt.json
uv run scripts/test-ws53-flash-uart-rx-qemu.py \
  --elf /tmp/ws53-uart-build/firmware.elf \
  --build-receipt /tmp/ws53-uart-build/receipt.json \
  --qemu /path/to/qemu-system-riscv32 --output /tmp/uart-normal
```

`--scenario blocked-route` 与 `--scenario cancel-peer-timeout` 各执行两轮故障后再执行
一轮恢复；输出目录必须不存在。Unix socket 路径应足够短。构建后输入有变化必须
重新构建，不能仅重写 receipt。构建/模型记录不是签名或敌对主机下的证明。

## 明确限制

该 Flash ELF 模型模拟已观察到的 FlashBoot 交接，不执行完整厂商启动链。
数字 pad 时间和软件超时不能证明电压、精确物理时钟或掉电启动。WS63 UART RX
模型仍缺少 FIFO 计数及完整 IRQ 行为，不能沿用 WS53 的模型结论。
旧未跟踪辅助脚本及原始证据继续保留；本项没有删除或重写旧验收记录。
