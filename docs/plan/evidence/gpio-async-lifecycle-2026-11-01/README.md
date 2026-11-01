# GPIO async 生命周期证据

HAL 三批分别保持 WS53 行为抽取公共核心、修复 WS63 每引脚隔离与取消、
区分仅 mask 与带 pending 确认的 disable。
主机逻辑测试归 HAL；本目录保存集成模型、旧版反例与远端消费者构建来源。
这些结果不代表实板验收，也没有扩大 BS21 async 能力。

`ws63-qemu-baseline.json` 是预期失败的旧版对照：同一夹具在空 IRQ 后错误完成。
`ws63-qemu-intermediate.json` 保存中间版本在 pending-peer 场景的失败。
另外三份具名模型报告通过，所有原始诊断保留。夹具源码 SHA-256 在 report.json，
不将旧物理结果继承到新行为。

复现普通模型：按 `examples/ws63-gpio-async-model/README.md` 构建并调用父仓
`scripts/test-gpio-async-qemu.py`。WS53 构建 ram_smoke 的 gpio-async-smoke feature，
同一运行器使用 `--chip ws53`。不会使用串口、探针或板卡。

远端消费者：将已跟踪的模型夹具 main.rs/build.rs 复制到本目录 remote-consumer
清单旁的 src/main.rs/build.rs，再执行 report.json 中命令。清单只引用精确远端
开发提交和 registry PAC，锁文件独立；这是开发集成验证，未发布依赖仍阻塞发布。
原始构建和测试日志在 report.json 指定的工作区外部验证目录，并保留哈希。
