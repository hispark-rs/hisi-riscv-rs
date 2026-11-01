# WS53 温度轮询 QEMU 夹具

本示例仅使用合成温度模型，不代表实板时钟、模拟供电或热管理共享资源交接。
不要加载到实板。默认程序仍是ram_smoke；该夹具不访问Flash。

从父仓库运行：

```sh
cargo build --locked --manifest-path examples/ws53/Cargo.toml -p ws53_temperature_control --release -Zbuild-std=core
uv run --script scripts/test-ws53-temperature.py --qemu /path/to/qemu-system-riscv32 --elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_temperature_control
```

一个ELF执行四种独立场景，运行器精确检查每种场景的三行UART结果和模型诊断：

- 输入933：返回原码和125C名义值；重复读取、归还token及重新接管都保留样本。
- 输入1023：原码完整保留；名义温度为None，不伪报SDK的125C饱和值。
- 首次采样前停钟：有限轮询超时，重试仍超时；恢复配置/重新接管不会重复启动。
- CONTROL故障为0xdead：构造及恢复拒绝未知配置，保留权限，不向传感器写寄存器。

运行器还通过只读QOM计数检查START次数：前三种均为1，未知配置为0；检查ELF的
RAM区间、ilp32f及无A扩展指令。模拟假设包括16点平均、256us采样、5ms上报周期、
READY保持及START重启。读到READY不等于新样本，时钟丢失后可能一直读到旧值；
独立寄存器模型测试覆盖此情况。本程序没有未经证实的stop/IRQ/清状态操作。

物理温度精度、采样时序、校准、低功耗及控制核热管理协调仍需后续验收。
