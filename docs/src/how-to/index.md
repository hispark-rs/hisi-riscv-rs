# 操作指南 · How-to Guides

这一章是**任务导向的菜谱**：每篇回答一个「如何做某件事」的具体问题，假设你已经掌握了基础（不懂概念请看[原理与背景](../explanation/index.md)，要查字段/地址/标记串请看[参考](../reference/index.md)）。每篇都给出可照做的步骤，并尽量覆盖真实环境里的变体、坑和排错。

## 构建 · Build

- [如何安装 hisi-riscv 工具链](install-toolchain.md) —— 下载/链接（或源码构建）自定义硬浮点 rustc 工具链，并验证 `riscv32imfc` target。
- [如何构建一个示例](build-example.md) —— 从仓库根工作区 `cargo build -p <name> --release`，ELF 落点，release/debug 与 objcopy 到 bin。

## 打包与烧录 · Flash

- [如何打包成可启动镜像（hisi-fwpkg）](package-image.md) —— `image`（裸 0x300 镜像）vs `pack`（fwpkg），0x300 header 是干嘛的。
- [如何用 probe-rs 烧录到真机](flash-probe-rs.md) —— 验证主路径：`image` → `probe-rs download` → `probe-rs reset`，补丁版 fork + yaml + 各芯片基址 + 排错。
- [如何用 hisiflash 烧录到真机](flash-hisiflash.md) —— 厂商 YMODEM 路径：`pack` → `.fwpkg` → `hisiflash flash`，何时用它。
- [如何用硬件 runner 让 `cargo run` 烧真机](hardware-runner.md) —— 用 `hil/cargo-run-hw.sh` 把 `cargo run` 从 QEMU 改成烧真机；全部环境变量。

## 测试 · Test

- [如何运行 HIL 冒烟测试](run-hil-tests.md) —— `hil/hil-smoke.sh` 逐示例的 UART 标记断言、环境变量、读懂通过/失败。
- [如何用 probe-rs 调试与读内存](debug-probe-rs.md) —— 用补丁版 probe-rs `read`、`reset_and_halt`、读 CSR/内存、用 HW 断点抓应用入口、dump ROM。

## 开发 · Develop

- [如何从模板新建一个工程](new-project.md) —— `cargo generate` 从 hisi-rs-template 起步，用生成的 justfile 完成首次构建+烧录。
- [如何新增一个外设驱动](add-driver.md) —— HAL 驱动模块范式、外设单例宏、sealed trait，以及配一个带 PASS 标记的 HIL 示例。
