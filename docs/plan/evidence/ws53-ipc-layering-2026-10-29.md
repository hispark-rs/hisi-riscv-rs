# WS53 IPC 分层迁移的软件验证

协作组件 `615fcf5`、HAL `6fa9738`：独立组件 27 项主机测试、`defmt`、
Clippy 和 RISC-V 检查通过。一个全新外部工程使用 Git 固定版本和空 Cargo 缓存
构建并验证编解码，没有父仓 patch；此结果不是 crates.io 发布验收。

重新构建的 IPC 模型测试结果：doorbell 3 轮、IRQ26 12 次/IRQ27 3 次，编解码
6954 帧；邮箱 3 轮、发送 309、接收 303、拒绝 57；SPSC 3 轮双向各 1536 帧；
IPC18 128 个场景。测试使用本地 QEMU 和虚拟对端，没有访问硬件。邮箱模型不
证明并发共享环，SPSC 模型也不能证明已安装控制固件兼容或电气/缓存行为。
上述四组测试仅覆盖直接队列路径。后续独立端点测试见下文。

原始报告保留于主工程本地 `target/`，校验值如下（SHA-256）：

| 报告目录 | SHA-256 |
| --- | --- |
| `ws53-coex-layered-base01` | `7d341cbfc9cf25864c91e2658310ea52c5b3e353f8932563208f9b92e3208c84` |
| `ws53-coex-layered-mailbox01` | `c7634caa00ca0f92ed601e47ec2246698fac0ef36af3a4496422f45e4632b752` |
| `ws53-coex-layered-spsc01` | `c0e324286c41bbcb9a4f6ec01a77ee0dac2ebc6c6893ad792156c4beb449a6f7` |
| `ws53-coex-layered-random01` | `258a457e55c24fa05c11418609272020d831e2b65bd23d70fcd93ecc85e02d4f` |

## 轮询端点并发模型回归

在协作组件 `615fcf5`、HAL `fba0a1f` 和 QEMU 模型 `fa47993` 上，分别编译
相同夹具的直接队列与 `Session<PollingEndpoint>` 配置。两者均运行三轮，每轮
双向各 512 条，队列满、回绕和无效消息错误检查通过，虚拟对端操作期间 CPU
未被逐次暂停。端点配置额外验证短缓冲保留及重试和显式失效；失效不等于
对端静止或共享区回收。直接队列配置仍测试显式对端静止后的资源回收。

虚拟对端只复制帧，不访问其地址或执行业务；因此测试策略不能用于物理设备。
QEMU 使用共享普通 RAM、假定的数字 IPC 模型，不证明实际固件兼容、缓存维护、
IRQ 竞争、通知唤醒或任何实板行为。未操作实板。报告分别保存在本地
`target/ws53-coex-endpoint-qemu-20261029/report.json` 与
`target/ws53-coex-direct-qemu-20261029/report.json`，SHA-256 分别为
`2008e91e7ab4284bbfa8edd60f1da66cf1e286d47f0931c29e7bcb987c60720c` 和
`8e868f4bcbb309a0116a8fd43cc48eaf32717bebcef7436ea72047b3af287422`。

复现时获取远端 QEMU 仓库的 `fa47993dae02a6e01ab0cab4e9d043592c11e3df`，
按 `scripts/ws53_ci_qemu_ipc_mask_build.py --help` 构建带来源凭证的 QEMU；
使用 `uv run scripts/ws53_ipc_endpoint_build.py --output <新目录>` 构建端点
ELF，再使用 `uv run scripts/test-ws53-ipc-spsc-model.py --endpoint --help`
查看 QEMU、来源、两份凭证、ELF 和输出目录参数。直接队列配置使用
`ws53_ipc_spsc_build.py`，运行时去掉 `--endpoint`。各输出必须使用新目录。
