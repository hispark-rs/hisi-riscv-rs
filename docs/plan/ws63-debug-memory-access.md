# WS63 调试系统内存访问诊断

## 状态

诊断工作已于 2026-07-13 完成。AP1 已被证明是可直接访问系统内存的路径，而
RISC-V SBA 未实现。AP1 当前仍是诊断结果，不是默认下载路径：probe-rs 需要先建立
明确的双 AP transport contract，才能同时安全持有用于 DMI 的 AP0 和用于系统内存的
AP1。本诊断已经**完成**；AP1 产品集成属于**条件触发的延期事项**，不是当前
connectivity 任务。

## D0：枚举 DAP

- 枚举所有 AP，记录 IDR、BASE、CFG、AP 类型和 ROM table。
- 分别识别 AHB-AP、AXI-AP、其他 MEM-AP 与 AP0 DMI aperture。
- 与 WS63 原厂 OpenOCD 配置对照；其他 HiSilicon 芯片的证据不能替代 WS63 证据。
- 验收：形成可复现、只读的 AP 清单，包含原始寄存器值和解码类型。

### 证据

完整扫描 APSEL `0..=255` 后只发现两个 ADIv5 AP：

| AP | IDR | BASE | CFG | 观测到的 CSW | 解码角色 |
| --- | --- | --- | --- | --- | --- |
| AP0 | `0x44770002` | `0x80000003` | `0x00000000` | `0x80000042`, `0x80000052` | APB3 Memory-AP，在 `0x80000000` 暴露 RISC-V DMI aperture |
| AP1 | `0x74770001` | `0x00000002` | `0x00000000` | `0x43800042`, `0x0b800052` | AHB3 Memory-AP，不带 CoreSight ROM table |

AP1 的 `BASE=0x00000002` 表示不存在 ROM table，并不表示 AP1 无法传输内存。WS63 原厂
OpenOCD 配置用 `-apsel 0 -dbgbase 0x80000000` 选择 AP0，这与 AP0 的 DMI 角色一致，
但它没有枚举 AP1。CSW 只作为观测结果记录，不当作不可变身份：调试操作配置
Memory-AP 时，其传输宽度、地址递增和保护字段都会变化。

## D1：安全验证地址映射

- 让暂停的 CPU 在保留 scratch SRAM 范围写入哨兵数据。
- 通过每个候选 AP 只读该范围；先测试有文档依据的 alias 或 offset，再考虑写入。
- 禁止探测任意外设、OTP、eFuse、安全窗口或 flash 控制窗口。
- 如果存在映射，记录 AP、offset、支持宽度、访问属性、cache/coherency 要求和实测吞吐。
- 验收：任何 AP 直接写入前，多次哨兵读取都必须与 CPU 视角一致。

### 证据

- hart 暂停后，CPU 调试路径在 scratch SRAM `0x00a70000` 写入 16 字节哨兵。AP1 在
  相同地址返回完全一致的数据，AP0 返回零；AP 侧不需要额外 offset 或 alias。
- AP1 的 8、16、32 位访问均返回一致的小端数据。
- 受控 AP1 写入和读回能被 CPU 调试路径观测到，随后已恢复原 scratch 内容。
- halt/write/read/restore/resume harness 对 4,096、65,532、65,536 字节分别连续通过
  三次；其中 65,532 字节覆盖了非 64 KiB 整数倍的边界。
- 在 2 MHz SWD 时钟下，AP1 持续吞吐约为 85-93 KiB/s。使用关闭 flashboot watchdog
  的 RF 镜像后，五次 64 KiB 读取和五次 256 KiB 读取全部完成。此前运行
  `uart_hello` 时的失败与继承的 watchdog 在长时间读取期间复位目标相关，并非 AP1
  地址或访问宽度错误。

实验只证明 hart 暂停时访问一致，尚未承诺 cache 与运行中 DMA 的 coherency。任何产品
写入路径都必须暂停 hart，或建立更强的目标专属 coherency contract。

## D2：验证 RISC-V SBA

- 解码 `sbcs` 的 `sbversion`、`sbasize`、`sbaccess32`、busy、error 和 autoincrement。
- 把原厂 OpenOCD 的 `riscv set_prefer_sba off` 视为风险信号。
- 只有声明的能力自洽时，才在 scratch SRAM 依次测试读取和受控写入/读回，并覆盖
  busy/error 恢复和吞吐。
- 验收：scratch 测试可重复且有显式错误恢复；仅存在寄存器不能证明 SBA 可用。

### 证据

AP0 暴露 `SBCS=0x20040000`，解码如下：

- `sbversion = 1`；
- `sbasize = 0`；
- `sbaccess8/16/32/64/128 = false`。

因此 Debug Module 没有声明 system-bus 地址宽度，也没有任何可用访问宽度。测试没有尝试
写入 `sbaddress*` 或 `sbdata*`。这与原厂 `riscv set_prefer_sba off` 一致：SBA 被判定为
不能用于 WS63 内存传输，而不只是优先级较低。

## 能力规则

任何由此产生的快速路径都必须是默认关闭的显式 target capability。成为 WS63 默认路径
之前，必须通过读写一致性、超时与恢复测试；小 UART 镜像和大 RF 镜像各重复下载三次；
保留完整验证，并取得 J-Link nRST 启动 marker。

现有 repeated-DMI DATA0 写优化属于另一条路径：它只加速已证明的 AP0 DMI aperture，
不声称直接访问系统内存。其 batch size 和 backpressure 仍是显式 WS63 target
capability。发现另一个 AP、alias 或 SBA 时，必须建立带独立 fallback 和证据的新能力，
不能静默替换当前 transport。

未来的 AP1 capability 至少要描述 DMI AP、system-memory AP、允许的 RAM 范围、支持
宽度、最大传输长度、halt/coherency 要求、超时和恢复行为。target 配置必须显式选择；
其他 `ArmWithRiscv` target 保持当前 progbuf/DMI 行为。不能把 AP1 藏在 WS63 flash
algorithm 内，也不能仅凭存在第二个 Memory-AP 就推断启用。

## 剩余集成工作

1. 为 `ArmWithRiscv` session 建立通用双 AP ownership model，不再让 DMI transport
   全生命周期独占 AP0。
2. 只把 target 声明的 RAM 范围路由到 AP1；寄存器、不支持的范围和没有该能力的 target
   继续回退到 progbuf/DMI。
3. 增加 transport 测试，覆盖能力未声明、范围非法、目标运行中、超时和部分传输恢复。
4. 启用 WS63 capability 前，对小 UART 镜像和大 RF 镜像重新执行完整下载、读回验证和
   J-Link nRST 启动检查。
