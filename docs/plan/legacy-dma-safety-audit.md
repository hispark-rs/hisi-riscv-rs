# WS63 / BS21 旧 DMA guard 安全审查

状态：**未修复，禁止把旧传输 guard 描述为已证明内存安全**。本记录针对
`hisi-hal` 的旧 `src/dma.rs`，不推断 WS53 `dma_ws53.rs` 有同样的行为。
本轮未操作板卡，也没有证明控制器总线事务的完成语义。

## 已从源码确认

1. `quiesce_channel` 在有限次数内等待 `active`，超时仍执行 disable 并返回。
   `Transfer::wait` 和两种 guard 的 `Drop` 之后可以释放或归还缓冲区及通道。
   因此软件没有证明该时刻停止访问内存；现有“provably stopped”“UAF
   unrepresentable”等说明超出源码能够支持的范围。
2. 通用 `embedded_dma` 缓冲区可携带非静态借用。安全代码可遗忘 guard，
   跳过其 `Drop`；仅靠 guard 保存借用不足以保证原缓冲区一直可供硬件访问。
3. WS63 在目标区完成或超时后按缓存行失效。接口只建议对齐，并未证明
   目标区首尾整个缓存行没有其他脏数据；失效可能丢弃相邻数据。
4. WS63 外设路径接受由调用方提供的地址、宽度及路由，需单独审计其
   类型和权限约束。不能把普通地址的数值有效性等同于设备及总线权限。

## 关闭条件

- 核对 WS63 / BS21 各自 SDK 和控制器证据，区分 channel enable、active、
  总线未完成事务及外设排空；在无 drain 证据时保留通道和缓冲区，不能
  以超时或禁用位变化作为归还许可。
- 设计对 `mem::forget` 仍成立的生命周期和所有权契约，及非一致性缓存
  完整行的资源约束；不把未经证明的前提简单包装成安全 token。
- 逐项审查内存、外设 TX/RX、失败恢复和掉电边界；为可复用的状态逻辑
  提供主机并发/错误注入测试，再按实际条件做 QEMU 与实板验证。
- 旧 `unstable` 接口如果调整，必须同步示例及消费者，明确兼容性与迁移；
  已发布的稳定 API 不得无故破坏。修复前不要新增依赖旧 guard 安全保证的
  示例，不将既有实板的正常完成结果当作取消/超时安全证明。

源码基线：HAL `3adeac4`；相关路径为 `start_mem_to_mem`、
`start_mem_to_peripheral`、`start_peripheral_to_mem`、`Transfer::wait`、
`PeripheralTransfer::wait`、`quiesce_channel` 和两个 `Drop` 实现。
