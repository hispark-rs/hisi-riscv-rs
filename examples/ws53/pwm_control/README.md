# WS53 PWM QEMU 夹具

仅用于 QEMU，**不要加载到开发板**。本例不验证安装固件的 RAM 所有权，也没有
物理 PWM 引脚映射；`OutputRoute` 使用的 pad 是无外设冲突的虚拟独占权限。

本例通过真实 HAL/PAC 运行四组八通道有限脉冲并进入 IRQ72，验证短块的
`FiniteDeadlineMissed`、占空比 trait、路由释放和 Drop 清理。`clock-stall-smoke`
变体在测试器断开模型时钟后验证没有完成事件且能清理；不宣称驱动已检测时钟故障。

在主工程根目录构建两个镜像并运行：

```sh
mkdir -p target/ws53-pwm
cargo build --locked -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml -p ws53_pwm_control --release
cp examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_pwm_control target/ws53-pwm/normal.elf
cargo build --locked -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml -p ws53_pwm_control --release --features clock-stall-smoke
cp examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_pwm_control target/ws53-pwm/stall.elf
uv run --script scripts/test-ws53-pwm.py --qemu "$WS53_QEMU" --elf target/ws53-pwm/normal.elf --clock-stall-elf target/ws53-pwm/stall.elf
```

`WS53_QEMU` 指向包含 PWM 模型的 `qemu-system-riscv32`。测试器显式启用
`ws53-pwm.assume-timing=on`，并检查 ELF、完整串口结果和预期诊断。模型假设
32MHz/16位计数、清计数提交立即生效、其他提交在块末生效，均未实板证明。
短块错误是保守检查：可能拒绝刚完成的块，也不能撤回已经输出的脉冲。
