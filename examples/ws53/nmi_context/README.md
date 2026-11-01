# WS53 NMI 软件上下文测试（仅 QEMU）

此程序主动写 `mcause/mepc/mstatus` 后跳转真实 direct trap 入口，验证运行时
的 NMI 分流、现场保存和 `mret` 返回。它**不会产生真实 CPU NMI**，不操作
NMI/WDT 寄存器，不证明硬件屏蔽、优先级、重投递、源清除或复位行为。
不允许把本程序加载到实板作为 NMI 验收。

从父仓库构建（示例 workspace 默认仍只构建原有 `ram_smoke`）：

```sh
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml \
  -p ws53_nmi_context --release --locked
cp examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_nmi_context /tmp/ws53-nmi-context.elf
cargo build -Zbuild-std=core --manifest-path examples/ws53/Cargo.toml \
  -p ws53_nmi_context --release --locked --features default-hook-test
uv run --script scripts/test-ws53-nmi-context.py --qemu /path/to/qemu-system-riscv32 \
  --elf /tmp/ws53-nmi-context.elf \
  --default-elf examples/ws53/target/riscv32imfc-unknown-none-elf/release/ws53_nmi_context
```

具名场景：

- 主循环的 3 种 FS 状态（Off/Clean/Dirty），返回时恢复 MIE=1。
- 真正 TIMER IRQ53 内的 3 次软件注入，恢复 MIE=0 和原 `mscratch`。
- 每个外层 hook 再调用一次软件注入，共 12 次；检查外层栈保护数据和返回。
- 检查全部整数/浮点寄存器、fcsr、栈指针、16 字节调用栈对齐，以及现场中的
  cause/mtval/mscratch。hook 故意修改部分浮点寄存器和 fcsr。
- 未提供 hook 的镜像通过 QMP 读取 CPU PC，确认停在 `__ws53_default_nmi`，
  不把“没有输出”当作默认处理器已验证。

运行时 `WS53_NMI(&NmiContext)` 是要求 `chip-ws53,unstable` 的 unsafe C ABI
hook；不启用普通 IRQ、不使用仅由普通临界区保护的共享借用、不获取被打断代码
持有的锁，引用不能逃逸。hook 返回前须由源所有者完成已验证的清除协议。
默认 hook 保持停止，不调用 SDK 地址冲突的 `OsClearNMI`。

默认 NMI 栈扩大为 2 KiB。测试只覆盖 hook 内的两层软件进入，未覆盖任意指令
边界重入、硬件再次投递或栈溢出；栈预算仍是应用约束。入口未占用 `mscratch`，
通过 callee-saved s0 保存当前 frame；重入已在 NMI 栈内时不会把 SP 重置到栈顶。
