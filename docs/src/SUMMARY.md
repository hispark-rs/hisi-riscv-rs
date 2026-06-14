# Summary

[引言](introduction.md)

---

# 教程 · Tutorials

- [本章导读](tutorials/index.md)
- [搭建开发环境](tutorials/01-setup.md)
- [点亮第一个 LED（blinky）](tutorials/02-blinky.md)
- [第一个 UART 程序（uart_hello）](tutorials/03-uart-hello.md)
- [在 QEMU 里运行与调试](tutorials/04-qemu.md)
- [第一次硬件在环测试](tutorials/05-first-hil.md)

# 操作指南 · How-to Guides

- [本章导读](how-to/index.md)
- [安装 hisi-riscv 工具链](how-to/install-toolchain.md)
- [构建一个示例](how-to/build-example.md)
- [用 probe-rs 烧录到真机](how-to/flash-probe-rs.md)
- [用 hisiflash 烧录到真机](how-to/flash-hisiflash.md)
- [打包成可启动镜像（hisi-fwpkg）](how-to/package-image.md)
- [用硬件 runner 让 cargo run 烧真机](how-to/hardware-runner.md)
- [运行 HIL 冒烟测试](how-to/run-hil-tests.md)
- [用 probe-rs 调试与读内存](how-to/debug-probe-rs.md)
- [从模板新建一个工程](how-to/new-project.md)
- [新增一个外设驱动](how-to/add-driver.md)

# 参考 · Reference

- [本章导读](reference/index.md)
- [内存映射](reference/memory-map.md)
- [示例目录与验证标记串](reference/examples.md)
- [HAL API 总览](reference/hal-api.md)
- [外设清单与覆盖情况](reference/peripherals.md)
- [工具链与编译目标](reference/toolchain.md)
- [应用镜像格式与签名](reference/image-format.md)
- [HIL 标记串与环境变量](reference/hil-markers.md)
- [CLI 工具速查（hisi-fwpkg / probe-rs）](reference/cli-tools.md)

# 原理与背景 · Explanation

- [本章导读](explanation/index.md)
- [系统架构总览](explanation/architecture.md)
- [启动流程：mask ROM → flashboot → app](explanation/boot-flow.md)
- [硬浮点工具链](explanation/hardfloat-toolchain.md)
- [async 与 embassy](explanation/async-embassy.md)
- [安全启动与签名](explanation/secure-boot.md)
- [QEMU 模型](explanation/qemu-model.md)
- [HIL 测试框架](explanation/hil-framework.md)
- [组件深入文档](explanation/components/index.md)
