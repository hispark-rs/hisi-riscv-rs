# 本章导读 · Reference

参考章节是**信息导向**的查阅资料：精确、结构化、可逐项检索。这里只**陈述事实**（地址、大小、标志位、字段偏移、签名、默认值），不讲教程，不讲原理。需要"怎么做"请看 [操作指南](../how-to/index.md)，需要"为什么"请看 [原理与背景](../explanation/index.md)。

本章所有事实均直接取自源码（`memory.x`、HAL 源文件、`hisi-fwpkg` 源、HIL 脚本、工具链配置）。

## 速查入口

| 页面 | 内容 |
|------|------|
| [内存映射](memory-map.md) | WS63 内存区域、导出的链接符号、栈大小、复位向量与入口 |
| [示例目录与验证标记串](examples.md) | 18 个示例的用途、观测通道、精确的成功标记串、是否需接线、QEMU/真机状态 |
| [HAL API 总览](hal-api.md) | `hisi-riscv-hal` 公开 API 结构图：驱动模块、构造函数、单例/GPIO/多实例/sealed/特性 |
| [完整 API 文档（rustdoc）↗](https://hispark-rs.github.io/hisi-riscv-rs/api/) | hal/pac/rt 的逐项 API；与本手册同站部署在 `/api/`，CI 自动构建 |
| [外设清单与覆盖情况](peripherals.md) | 全部 HAL 驱动模块、外设、基地址、示例覆盖、可否裸板自检 |
| [工具链与编译目标](toolchain.md) | `hisi-riscv` 工具链通道、目标三元组、`rust-toolchain.toml`、`.cargo/config.toml` |
| [应用镜像格式与签名](image-format.md) | 0x300 镜像头字段布局、fwpkg V1 容器、CRC16 |
| [HIL 标记串与环境变量](hil-markers.md) | 每个示例的 HIL 标记串、HIL 脚本消费的全部环境变量 |
| [CLI 工具速查](cli-tools.md) | `hisi-fwpkg`、补丁版 `probe-rs`、QEMU、`hisiflash` 命令与仓库清单 |
