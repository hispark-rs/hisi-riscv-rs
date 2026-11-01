# `block_on` 软件唤醒反例

这是一条**已复现、尚未修复**的问题证据，不是通过的功能验收。
HAL 固定到 `c907bed`；同一个 Future 第一次 poll 调用 `wake_by_ref` 后 Pending，
第二次必定 Ready。原 block_on 未输出 COMPLETE；两次显式 poll 的诊断对照输出
COMPLETE。观察窗口均为 3 秒，两个测试程序最终均不会自行退出。

构建（在 fixture 目录，工具链 nightly-2026-07-09）：

```sh
cargo build --locked -Zbuild-std=core,alloc --target riscv32imfc-unknown-none-elf --release
cp target/riscv32imfc-unknown-none-elf/release/block-on-wake-counterexample ../counterexample.elf
cargo build --locked -Zbuild-std=core,alloc --target riscv32imfc-unknown-none-elf --release --features poll-control
cp target/riscv32imfc-unknown-none-elf/release/block-on-wake-counterexample ../control.elf
```

在本目录执行 `uv run --script observe.py --qemu <binary> --counterexample
counterexample.elf --control control.elf --output <new-report.json>`。
观察器拒绝覆盖原报告；返回零只表示旧缺陷复现，报告始终明确
`qualification_passed=false`、`defect_fixed=false`。修复后原反例应不再复现，
届时应新增功能验收而不是修改历史报告。

ELF 使用固定远端开发依赖和独立锁文件，未混入父仓 patch。
只运行 WS63 QEMU，不操作硬件，不将其推广为 WS53/BS21 实测。
没有注入 poll 后、WFI 前的真实 IRQ；那个窗口仍需独立验证。
诊断对照不是忙轮询执行器的产品实现，不替代后续平台休眠契约。
