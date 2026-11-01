# HAL 独立主机测试门控

HAL `d5ca0ba` → `a52e0dd`；修改只位于测试、子仓检查脚本及说明文档，生产驱动
代码与 API 不变。先前 16 项 WS63 主机测试 Clippy 问题，以及新增三芯片门控发现的
两项 BS21 问题均关闭。没有降低 `-D warnings` 或增加全局 lint 豁免。

子仓脚本从已跟踪源码创建父仓外副本，使用精确远端 runtime 开发覆盖；主机测试、
主机测试 Clippy、目标 Clippy、文档及错误 feature 组合检查均通过。
WS53/WS63/BS21 主机测试分别为 445/372/220 项，数量未减少。普通编译不依赖
本机绝对路径；开发覆盖仍不代替 registry-only 发布验收，也不宣称空缓存构建。

## 测试有效性

DMA 的属性测试此前重新实现编码器。现在直接调用生产 `control_word` 和
`channel_config_word`，用独立字段位号/保留位断言检查输出；尺寸还经过实际
`DmaTransferSize` 边界。零仍是低层类型允许的值，不偷改其语义。
PWM 针对全部 u8 值调用真实 `PwmChannelId::from_index` 检查合法值与拒绝路径。

在两个隔离副本分别使用旧、新测试，对生产编码器施加相同
[两行故障](dma-encoder-mutation.patch)：源 burst 左移 12→13，bus-lock 位 14→15。
设置 `PROPTEST_CASES=512 PROPTEST_RNG_SEED=20261101` 后运行
`cargo test --target x86_64-unknown-linux-gnu --no-default-features
--features chip-ws63,unstable,async --lib dma::proptests::`。
旧测试 12/12 通过，新测试 9 通过、3 失败；三个最小反例见日志。
这证明这些属性对真实编码错误敏感，不证明整个测试集或硬件均正确。
故障只在临时副本中，未进入工作分支；不得将故障副本用于目标构建或实板。

当前源码的正常测试和最终独立门控均通过。初次独立门控因 BS21 两项问题失败的
日志保留，不把首次失败覆盖为成功。材料位于工作区同级
`validation/hal-test-lints-2026-11-01/`，原始文件哈希索引见 `report.json`。
本目录两个公开 mutation 日志仅清除行末空白；原始日志保持原样。
本轮未运行新的 QEMU/实板测试；无生产行为变化无需重复既有硬件测试，但 WFI
交接、DMA 缓存/停止、跨核资源、Flash/冷启动和发布等原有未关闭项仍保留。
