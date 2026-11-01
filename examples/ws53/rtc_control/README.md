# WS53 RTC — QEMU only

此例只验证两路RTC的Rust HAL与明确假设时序的QEMU模型，**不能加载到当前实板**。
没有证明低速时钟、共享资源、实际中断掩码极性或恢复路径。它不是默认构建目标。

三个独立镜像由测试器执行六种场景：

- 普通镜像分别使用请求位读回零/保持旧值，验证两路计数、单次/周期IRQ49/50、
  自由运行回绕，以及已经挂起的中断在stop/Drop后不再投递。
- `timeout-smoke` 分别停止两路计数时钟或阻塞取样握手，必须返回SnapshotTimeout。
  每路65535次轮询期间，TIMER0 ISR必须实际执行，且观察到只暂停当前RTC的本地
  IRQ、另一RTC仍使能；返回后全局中断、源使能与P3优先级均保持。
- `expiry-race-smoke` 在CONTROL读和取样请求写之间强制单次到期，两种读回设置
  都须返回Expired、停止意外重启，同时保留原中断，两路各投递一次。

原SDK式CONTROL读改写可能把旧ENABLE写回、重启已到期计数。Rust现在只暂停当前
RTC本地IRQ，完成请求及raw-pending检查，避免该ISR过早清走证据；不全局关中断
轮询。检测到单次到期后只停止计数，不吞掉中断，恢复源使能后再等待32us。
到期场景从一开始就打开两路RTC及全局中断，检查恢复后各投递一次且P3优先级不变。
这个验证证明模型场景下的修复，
不能证明实芯片状态跨时钟域传播的时序。普通取样成功零值仍不同于到期或失败。

同步延时通过独立32MHz TIMER通道1完成。两路RTC的延时对象借用同一个RefCell，
只在主循环短暂读取TIMER；RTC ISR不借用计时器或主循环RtcDriver。所有等待有限。

从父仓库构建并分别保存（feature分别构建，不要同时启用；不要使用cargo run）：

```sh
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml -p ws53_rtc_control --release --locked
cp examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_rtc_control /tmp/ws53-rtc.elf
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml -p ws53_rtc_control --release --locked --features timeout-smoke
cp examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_rtc_control /tmp/ws53-rtc-timeout.elf
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml -p ws53_rtc_control --release --locked --features expiry-race-smoke
cp examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_rtc_control /tmp/ws53-rtc-expiry.elf
uv run --script scripts/test-ws53-rtc.py --qemu /path/to/qemu-system-riscv32 \
  --elf /tmp/ws53-rtc.elf --timeout-elf /tmp/ws53-rtc-timeout.elf --expiry-elf /tmp/ws53-rtc-expiry.elf
```

QEMU必须包含 `ws53-rtc` 及 `expire-on-request` 注入入口。测试器检查ELF、精确
UART标记及每路一次的假设时序诊断，其他未实现访问即失败。默认模型不会偷偷
采用猜测的RTC计数/掩码行为；测试器显式开启32768Hz、count+1到期、使能重新
装载、高有效mask和自由模式回绕假设。计数启动在模型中立即生效，65/32us等待
不是实测启动/停止延迟。实板20轮RAM/IRQ验收、低功耗和控制核交接仍未完成。
