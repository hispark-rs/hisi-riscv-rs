# Hi3322 runtime 移植预研

这页解释 Hi3322 不能直接塞进 WS63/BS2X runtime adapter 的原因。它不是支持清单，也不是操作指南；它记录后续适配前必须尊重的平台事实。

## 事实来源

本页只使用本地 vendor tree 中的 Hi3322 平台代码：

- `/Users/sanchuan/Documents/hispark/hs-fbb/src/kernel/seliteos/target/3322/reset_vector.S`
- `/Users/sanchuan/Documents/hispark/hs-fbb/src/drivers/chips/3322/arch/riscv/riscv70/vectors_tee.s`
- `/Users/sanchuan/Documents/hispark/hs-fbb/src/drivers/chips/3322/arch/riscv/riscv70/interrupt.c`
- `/Users/sanchuan/Documents/hispark/hs-fbb/src/drivers/boards/3322_evb/linker/seliteos/seliteos.prelds`
- `/Users/sanchuan/Documents/hispark/hs-fbb/src/tools/pkg/chip_packet/3322/packet.py`

## 和 WS63/BS2X 的关键差异

Hi3322 的 SELiteOS 启动路径不是普通的 `mtvec`/`mstatus` 裸机 reset path。`reset_vector.S` 设置 `tmtvec = TrapVector + 3`，配置 `tmedeleg`、`tmesvec`、`tmestop`，并通过 `tmstatus` 控制中断状态。`vectors_tee.s` 的 trap 入口同样围绕 TES/TEE CSR 展开。

中断控制也不是只靠 WS63 当前的 direct-mode `mcause` dispatch。`interrupt.c` 使用 CLIC 寄存器区域配置 `sys_clic_intie` 等位，并根据是否启用 TEE 在 `mstatus` 与 `tmstatus` 之间切换。

内存布局也不同。`seliteos.prelds` 把 flash、ITCM、DTCM 拆成 kernel、driver box、user text/data、shared data 等区域；这不是把 `memory.x` 地址替换掉就能复用的 WS63 layout。

## riscv-rt 的复用边界

可以优先复用：

- `#[entry]` / `#[pre_init]` 这类 Rust 入口属性；
- 标准 RISC-V 数据段/BSS 初始化约定；
- `memory.x`/linker contract 中可以保持芯片无关的部分。

不能强行复用：

- TES/TEE reset 序列；
- `tmtvec`/`tmstatus` trap 路径；
- CLIC 委托与中断使能；
- SELiteOS 多域内存布局；
- Hi3322 镜像/分区打包规则。

因此 Hi3322 需要独立 startup adapter。只有在确认某个裸机非 TEE 场景确实匹配普通 RISC-V `_start` 时，才能让该 adapter 选择更深地复用 `riscv-rt`。

## 后续适配门槛

新增 `chip-hi3322` 之前必须先补齐：

- Hi3322 PAC 或等价的寄存器事实来源；
- linker memory/layout 规格；
- reset/trap/interrupt adapter；
- 镜像打包规则与烧录路径；
- 至少一个板级 smoke 或 HIL 证据。

在这些证据存在前，`hisi-riscv-rt` 只保留 Hi3322 文档占位，不暴露可启动 feature。
