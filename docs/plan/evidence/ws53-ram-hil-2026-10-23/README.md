# WS53 RAM 启动与中断：历史实板摘要

本目录只保存经原始校验表核对的精简证据，不包含原始串口数据、板卡备份或固件。
原始材料保存在本地同名证据目录；每轮原始报告的 SHA-256 列于
[`rounds-summary.json`](rounds-summary.json)，可用于核对归档完整性。

- [`summary.json`](summary.json)：20 轮范围、控制核状态及未验收项目。
- [`rounds-summary.json`](rounds-summary.json)：逐轮通过、恢复和镜像读回结果，以及来源哈希。
- [`guard-tests.json`](guard-tests.json)：不连接硬件的 J-Link 自动更新与 Flash 辅助禁用守卫测试。

原始校验表中的摘要、矩阵、守卫测试和 20 轮报告哈希均已核对；此检查不替代
重新运行实板测试。控制核在实验中保持复位，不能据此证明双核共享资源交接、
真实断电冷启动或其他外设功能。当前状态以 [WS53 实施记录](../../ws53-adaptation.md) 为准。
