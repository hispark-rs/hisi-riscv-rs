# WS53 IPC 分层迁移的软件验证

协作组件 `615fcf5`、HAL `6fa9738`：独立组件 27 项主机测试、`defmt`、
Clippy 和 RISC-V 检查通过。一个全新外部工程使用 Git 固定版本和空 Cargo 缓存
构建并验证编解码，没有父仓 patch；此结果不是 crates.io 发布验收。

重新构建的 IPC 模型测试结果：doorbell 3 轮、IRQ26 12 次/IRQ27 3 次，编解码
6954 帧；邮箱 3 轮、发送 309、接收 303、拒绝 57；SPSC 3 轮双向各 1536 帧；
IPC18 128 个场景。测试使用本地 QEMU 和虚拟对端，没有访问硬件。邮箱模型不
证明并发共享环，SPSC 模型也不能证明已安装控制固件兼容或电气/缓存行为。
新 `PollingEndpoint` 另有主机测试，但这些 QEMU 场景尚未覆盖该新接口。

原始报告保留于主工程本地 `target/`，校验值如下（SHA-256）：

| 报告目录 | SHA-256 |
| --- | --- |
| `ws53-coex-layered-base01` | `7d341cbfc9cf25864c91e2658310ea52c5b3e353f8932563208f9b92e3208c84` |
| `ws53-coex-layered-mailbox01` | `c7634caa00ca0f92ed601e47ec2246698fac0ef36af3a4496422f45e4632b752` |
| `ws53-coex-layered-spsc01` | `c0e324286c41bbcb9a4f6ec01a77ee0dac2ebc6c6893ad792156c4beb449a6f7` |
| `ws53-coex-layered-random01` | `258a457e55c24fa05c11418609272020d831e2b65bd23d70fcd93ecc85e02d4f` |
