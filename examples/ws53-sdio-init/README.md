# WS53 SDIO 初始化模型示例

本例使用实际 `hisi-hal::sdio::Initialization`，检查外部主机时钟等待、
Function1使能、消息观察、超时、未知事件和取消。它是独立workspace中的
**QEMU测试程序**，不具备板级资源交接，不能直接烧录或加载到开发板。

在主仓库根目录构建（RAM程序区仅32KiB，使用release配置）：

```sh
cargo build --locked --offline --release -Zbuild-std=core,alloc \
  --manifest-path examples/ws53-sdio-init/Cargo.toml \
  --features model-handoff --bin sdio-init-model \
  --target riscv32imfc-unknown-none-elf
uv run --script scripts/test-ws53-sdio-init-model.py \
  --qemu /path/to/sdio-capable/qemu-system-riscv32 \
  --elf examples/ws53-sdio-init/target/riscv32imfc-unknown-none-elf/release/sdio-init-model \
  --output target/sdio-init-new-run
```

QEMU需包含`ws53-sdio`设备。工具显式启用`assume-init-profile`，通过虚拟主机输入
提供时钟和事件；Rust自行设置两个时钟门控和初始化寄存器。主机只向ELF符号声明
的普通RAM邮箱写入测试步骤命令，不预写SDIO寄存器、不更改CPU寄存器。
因此这是分步CPU测试，不能算作自主启动或完整SDIO主机枚举。

正常路径要求先等待时钟、应用配置，再收到新的FunctionEnable事件；旧READY值
不能替代本轮事件。等待使用跨阶段的总poll次数上限，超时/异常后保留句柄；
后续调用返回Poisoned。取消只屏蔽已知全局来源，不声称复位或排空ADMA。

当前HAL保持全局SDIO来源及调用者的本地IRQ46处于屏蔽状态，使用轮询服务事件。
模型的IRQ46输出另有qtest检查，尚不等于Rust IRQ处理器或实板中断已验收。
CIS、切换电压、控制器完整复位协议和真实主机收发仍待实现。
所有API继续处于`chip-ws53,unstable`；本例不改变其他Flash候选的资格。

现有 WS53 CI 已加入此独立workspace的release构建、RAM ELF契约检查和ELF归档。
模型执行仍使用上面的独立命令：新增QEMU提交尚未发布，CI的既有模型固定版本
保持原值，因此暂未将这10个场景接到远端QEMU任务。本地运行结果不等同于远端CI通过。

## ADMA所有权与收发模型

另一个程序`sdio-transfer-model`使用生产`Transfer`，在有界等待主机请求后提交
Rust构造的描述符。HostRead在准备好保留内存后，先写长度再写读回值OR1的通知命令；
HostWrite不发读通知。静态数据、表和尾随字在完成、失败及取消后均继续保留。
需要独立证明总线排空和缓存同步，才允许调用unsafe释放入口。取消只停止软件观察，
不能撤回已经接受的总线写入。本轮询示例保持IRQ46屏蔽，不执行异步传输。
HAL另有`async`功能下的实验事件等待适配器，其独立IRQ46模型验证见下节。

```sh
cargo build --locked --offline --release -Zbuild-std=core,alloc \
  --manifest-path examples/ws53-sdio-init/Cargo.toml \
  --features model-handoff --bin sdio-transfer-model \
  --target riscv32imfc-unknown-none-elf
uv run --script scripts/test-ws53-sdio-transfer-model.py \
  --qemu /path/to/adma-capable/qemu-system-riscv32 \
  --elf examples/ws53-sdio-init/target/riscv32imfc-unknown-none-elf/release/sdio-transfer-model \
  --output target/sdio-transfer-new-run
```

测试显式启用`assume-adma-profile`，假设传输由CMD53长度终止；SDK普通0x21尾描述符
本身不能证明这一点。14个场景覆盖双向64字节数据、超时、长度/方向拒绝、总线错误、
时钟丢失、未知/复位事件、完成通知丢失、取消后的延后写入，以及已发读通知后取消、
主机随后请求但CPU不再提交表。主机只写命令邮箱，
从调试接口检查数据和保护字节；CPU移交后不读取或回收DMA内存。
这是可控虚拟对端测试，未验证CIS、物理SDIO时序、缓存一致性或实板ADMA排空。

## IRQ46异步事件等待

`sdio-async-model`运行生产`Transfer::wait_async`和独占`on_async_interrupt`，
由真实CPU中断入口更新IRQ计数、waker只调度主循环。主机仅写测试命令邮箱及虚拟
对端输入；不直接调用ISR、不预写MMIO/CPU寄存器。每个事件等待返回完整快照，
包括消息、请求、完成或错误；取消后不回收静态数据或描述符。

```sh
cargo build --locked --offline --release -Zbuild-std=core,alloc \
  --manifest-path examples/ws53-sdio-init/Cargo.toml \
  --features model-handoff,model-async --bin sdio-async-model \
  --target riscv32imfc-unknown-none-elf
uv run --script scripts/test-ws53-sdio-async-model.py \
  --qemu /path/to/adma-capable/qemu-system-riscv32 \
  --elf examples/ws53-sdio-init/target/riscv32imfc-unknown-none-elf/release/sdio-async-model \
  --output target/sdio-async-new-run
```

15个场景覆盖双向数据和IRQ唤醒、消息后继续收发、请求先到、无请求取消、通知后
取消、已接受写入在取消后继续完成、双向总线错误、缺失完成通知、停钟、长度不符、
未知事件（使能/屏蔽分别验证）与功能复位。未poll的Drop和本地IRQ已使能时拒绝
绑定也逐场景验证。未知使能位来自模型保留的种子，不推断硅片保留位行为。

该程序依赖显式的模型交接，不能直接部署到开发板。模型仍以CMD53字节数结束
ADMA，不代表物理线路、CIS枚举、缓存一致性、控制核交接或总线排空通过。
CI构建和归档此ELF；所需QEMU提交尚未发布，远端模型运行尚未接入。
