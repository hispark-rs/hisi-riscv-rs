# WS53 Flash 启动实验

本目录独立于 RAM 示例工作区，避免 Cargo feature 合并把 RAM 调试程序误链接到
Flash。当前为开发稿：编译、布局检查和限定的 QEMU FlashBoot 交接测试通过，
已用 hisiflash 开发例程烧录候选02并完成整片回读、串口复位/物理SRST启动和DSLogic D5波形对照。
用户确认真正断电5秒后，未复位/加载RAM的事后CPU/RAM检查证明程序完成32次中断，
Flash回读与候选02一致；USB串口重枚举期间未捕获早期日志，**冷启动UART/波形仍待验收**。
正式CLI已完成原应用恢复和候选02重新烧录，两者均经整片校验及运行检查。
随后候选03（TCXO非阻塞配置）也完成真实烧录、整片回读、串口复位及两轮物理SRST，
UART/DSLogic/原始TCXO样本均通过；其自身四扇区恢复也已实测通过。
候选04（TCXO＋自主PWM）已完成正式CLI七扇区烧录、整片回读、串口复位、两轮
物理SRST和原应用恢复。候选05实烧发现RTC观察窗口累积执行开销的问题，已退役；
候选06改用TIMER1实际时间截止，已通过正式CLI烧录、整片回读、串口复位和两轮
物理SRST；三轮UART、DSLogic RTC/TIMER及PWM波形、TCXO样本均通过。
自身七扇区恢复、整片回读、原应用AT及SWD亦已通过，随后恢复原固件。
真正断电冷启动仍未验收。
候选07增加自主30秒采集准备窗口，已完成三档QEMU正常回归、停表故障检查和
正式CLI八扇区烧录、整片回读、串口复位及两轮物理SRST。三轮UART、D5 RTC/TIMER、
D11 PWM及30秒TIMER1诊断均通过，前后SWD各600次零错并可单步/继续。
**A板当前运行候选07；其自身恢复和真正断电冷启动仍待验收**。
具名报告见适配记录。

## 可选异步 TIMER 回归（候选08，仅软件验证，未烧录）

`timer-async-smoke` 显式启用真实异步 Future 测试，不属于默认配置，也不会由
`capture-delay` 隐式打开。它依赖 TCXO 预检，复用 RAM 的两路 TIMER 测试：
16 次到期、到期前/到期后的 4 次取消、6 次非法绑定拒绝及 44 个 LOAD 读回值。
测试不读取 UART 输入，不使用 WFI，不代表已支持低功耗。TCXO 时钟已检查后，
Flash Future 每次最多等 20ms（最长正常等待为 5ms），另有有限迭代防止独立
计数器也停止。RAM 原入口仍用无操作观察器，并保持原有 UART 结果与 IRQ 路由。

```sh
cargo build --locked --offline -Zbuild-std=core \
  --manifest-path examples/ws53-flash/Cargo.toml --release \
  --features capture-delay,timer-async-smoke \
  --target-dir target/ws53-flash-timer-async
```

去掉准备窗口时显式使用 `rtc-smoke,timer-async-smoke`。完整测试顺序为
TCXO → 异步 TIMER → PWM → RTC → 原有 32 边沿 TIMER，之后停止相关中断。
异步阶段独占两路 TIMER/IRQ53/IRQ54，退出时检查停止、单次模式、屏蔽中断、
无 raw/local pending 及全局 MIE 关闭，再交给后续测试。控制核仍保持复位；
这不是跨核共享 TIMER 的交接实现。

MIO17/DSLogic D5 以最低驱动强度持续保持输出；每路依次产生 7 个高脉冲，
最小窗口为 `1000/3000/5000/20000/1000/6000/1000µs`，分别对应三种等待、
到期前取消后观察、复用、到期后取消后观察及再次复用。UART 阶段标记在低电平
期间发送并等待 FIFO/移位寄存器排空，然后才拉高。脉宽包含主循环配置、轮询、
IRQ 和观察开销，**不是 ISR 精确响应时间**；20ms/6ms 取消窗口也包含设置和
清理开销，不能要求与标称时间逐 tick 相等。完整组合为 28 个异步阶段边沿、
8 个 RTC 边沿和 32 个 TIMER 边沿，共 68 个。真实波形仍需 DSLogic 验收。

`WS53_FLASH_TIMER_ASYNC_STATE` 保存停止时的两路到期计数、LOAD、控制、raw、
local enable/pending 与 MIE；`WS53_FLASH_TIMER_ASYNC_STAGE` 保存当前窗口。
两者先写内容、执行 fence、最后发布 phase，仅用于停住 CPU 后诊断。
已有候选07冻结 ELF、扇区计划与固定 HIL 资格不接受这个新组合。新增组合的
真实烧录、复位、冷启动和恢复均须独立验收，不继承候选07或 RAM 的实板结果。

候选08的冻结ELF为`4baa083b2de7440e406a15472c092430ef786716ee1d218ea8ca2370a599bdea`，
两次独立构建一致；正文33820字节。相同ELF在QEMU中正常与两路分别停钟各两轮
通过。正常路径不使用GDB；故障路径在明确READY位置停住模型，再设置对应的
`timer0-stall`/`timer1-stall`属性，严格核对实际Future的超时来源及未出现后续PASS。
此故障注入只验证软件和模型，没有在实板停止时钟。

```sh
uv run --offline --script scripts/test-ws53-flash-qemu.py \
  --qemu "$WS53_QEMU" --elf "$WS53_ASYNC_FLASH_ELF" \
  --test-profile tcxo-timer-async-pwm0-rtc --capture-delay --icount-shift 8 \
  --output "$WS53_NEW_MODEL_DIRECTORY"
```

故障场景在独立输出目录增加`--timer-async-fault stall0`或`stall1`；故障报告不能
作为烧录资格。实际生成的D5脉宽逐窗检查，后继RTC/TIMER与D11 PWM继续严格验证。
证据见[同ELF模型归档](../../docs/plan/evidence/ws53-flash-timer-async-model-2026-10-24/README.md)。
工作流已加入两种async配置构建与133项主机拒绝检查，本地全新构建/重放通过；
远端CI尚未运行，也没有更换其旧QEMU固定版本。

`hil/ws53-flash-cli.py --profile timer-async08`独立固定新ELF、正文和十扇区计划：
0x5d000..0x65000九个正文页、最后写0x30000组合头。离线检查和206项资格拒绝测试
通过；候选07仍保留在板上，新镜像尚未实烧。须先完成07本轮冷启动观察与自身
八扇区恢复，再验证原应用基线。08自己的十扇区恢复目标也是原厂固件，不能复用
07计划。公开[离线报告](../../docs/plan/evidence/ws53-flash-timer-async08-offline-2026-10-24/summary.json)
只包含元数据，不含厂商备份、Loader或扇区内容。

```sh
cargo build -Zbuild-std=core --manifest-path examples/ws53-flash/Cargo.toml --release --locked
uv run --script scripts/check-ws53-flash-elf.py \
  examples/ws53-flash/target/riscv32imfc-unknown-none-elf/release/ws53_flash_smoke \
  --binary /tmp/ws53-flash-smoke.bin
uv run --script scripts/test-ws53-flash-qemu.py \
  --qemu /path/to/qemu-system-riscv32 \
  --elf examples/ws53-flash/target/riscv32imfc-unknown-none-elf/release/ws53_flash_smoke \
  --output /tmp/ws53-flash-qemu-new
```

产物只是从 `0x45d300` 执行的应用正文，**不是完整烧录包**。组合头、正文摘要及
保留控制核的扇区计划由 `hisi-fwpkg` 从已经验证的整片备份生成，写入/回读由
`hisiflash` 完成。不能用 `objcopy` 后的正文覆盖整个应用/控制核组合分区。

`hisi-riscv-rt/ws53-flash-start` 要求 `chip-ws53 + unstable`，专用于已观察的
原厂 FlashBoot 交接：控制核保持复位，DTCM 配置 `0x20`，共享 RAM `0x3f0`，
MCU 模式字段为零。早期使用 `0x2f000..0x30000` ITCM 栈，以 PAC 设置模式1
内存布局后才使用应用 DTCM、复制 `.data` 并清零 `.bss`。其他交接状态拒绝执行，
不尝试猜测/修复。没有覆盖原厂控制核 Flash 内容，也尚未启动控制核或提供 IPC。

示例使用原厂 FlashBoot 配置的 UART2 和 TIMER 时钟，在入口后停止七秒看门狗，
验证初始化数据、真实定时器中断和 MIO17/D5 的 32 次翻转（10ms 间隔）。这一路
只允许连接逻辑分析仪输入。驱动最低强度，最后停在低电平。没有对所有 IO 扫描。

UART/时钟目前继承**板上原厂 FlashBoot**，不依赖调试器加载 RAM 或初始化。
完整自主时钟/缓存/控制核协作仍是后续适配项；本实验不宣称替代 BootROM 或
模拟完整厂商启动链。Flash 实板和真正断电冷启动应单独记录，不能用 RAM/QEMU
通过替代。`hil/ws53-ram-smoke.py` 的 ELF 检查应拒绝本镜像。

QEMU 必须含 `-M ws53,flashboot-handoff=on` 模型。测试脚本先确认 RAM 保持非零
初值，再运行真实 ELF，检查 data/bss、内存门控交接、控制核仍复位、停止看门狗、
IRQ53 和 UART 文本；两次启动均要求 pad17 输出32次交替跳变、间隔10ms±2%。
`pads.csv` 为数字模型的虚拟时间记录（ns、SDK pin、level、driving、floating、
contention），供后续与 DSLogic D5 的原始采样对照。不能据此声明真实电压或时序。
UART 文本检查也不等于 UART 线上的协议解码。看门狗计时仍为明确假设的模型。

2026-10-24 仿真发现并修复等待循环的双重计数读取竞态；旧候选01的 ELF
`51b6ecd8…` 和扇区计划仅作失败追溯，不再作为烧录候选。修正后的基础候选02 ELF
为 `96a3de5cb386768d353fa9fb68fd14c14e0d57516f358fb54fb04e0ae2c80986`；
各后续配置分别使用自己的冻结镜像和资格记录。


实板流程由 `hil/ws53-flash-application.py` 编排，调用 hisiflash 开发示例
`ws53_application`，不等同于通用 `hisiflash flash` 的 WS53 支持。
新增 `hil/ws53-flash-cli.py` 接到正式实验入口 `hisiflash ws53 flash|recover`，
冻结CLI、独立核对扇区计划及整片预期内容、在DSLogic READY后放行复位；
命令和必需文件见 `hil/README.md`，主机测试不能代替实际烧录/恢复结果。
`hil/ws53-flash-boot-observe.py --srst` 重测物理复位与波形；默认模式只监听
串口等待外部断电，报告不会自动把一次新日志标为真正冷启动。
详细哈希、备份/恢复及验收范围见 `docs/plan/ws53-adaptation.md` 最新记录。

## TCXO Flash 测试配置

`tcxo-snapshot-smoke` 复用 RAM 示例的 TCXO/TIMER1 测试：保留计数，采集六个
64 位样本，对照 TIMER1 IRQ54 的 10/20/40ms，并连续取样32次。
`tcxo-nonblocking-smoke` 额外检查40次非阻塞采样、32次缓存读取、8次取消重用
及16次拒绝路径。两种配置都先完成 TCXO 检查，再输出原有的 D5/IRQ53 完成波形。
只在已确认应用核独占快照窗口、32MHz计数时钟且无睡眠的条件下使用；现已在
控制核保持复位的Flash交接配置实测，生产IPC交接和低功耗仍未验证。

```sh
cargo build -Zbuild-std=core --manifest-path examples/ws53-flash/Cargo.toml \
  --release --locked --features tcxo-nonblocking-smoke
uv run --script scripts/test-ws53-flash-qemu.py \
  --qemu /path/to/qemu-system-riscv32 \
  --elf examples/ws53-flash/target/riscv32imfc-unknown-none-elf/release/ws53_flash_smoke \
  --test-profile tcxo-nonblocking --output /tmp/ws53-flash-tcxo-new
```

QEMU 检查非零高32位的完整原始样本，另以 `--tcxo-fault wrong-frequency`、
`clock-off`、`stall` 分别注入错误频率、停钟和取样不完成。故障报告的
`passed=true` 只代表正确检出故障，`application_completed=false`，不能用于
烧录资格证明。每个场景均运行两次，包括一次模型复位。

默认配置提取的应用正文仍与已烧录候选02逐字节相同；新增非阻塞配置正文为
9412字节，离线镜像计划涉及 `0x5d000/0x5e000/0x5f000` 和最后写入的
`0x30000` 组合头四个扇区。它已通过独立`tcxo03` HIL配置实际烧录，不能套用
候选02的三扇区HIL资格。三个实板启动场景的10/20/40ms样本增量均为
324197/644260/1284394ticks，满足5%功能检查；多出的约131–137µs包含取样和
IRQ/执行开销，未单独分离，不宣称与QEMU逐tick相同或具备精确时间精度。
同轮D5实测32次翻转，平均间隔10000.355–10000.387µs；前后SWD各600次零错误。
候选03自身四扇区恢复现已通过正式CLI实测：整片回读、现场NV保持及原应用AT
均通过，见`docs/plan/evidence/ws53-flash-tcxo-recovery-2026-10-24/summary.json`。
真正断电冷启动、TCXO跨核交接/低功耗仍未验收。
CI现包含五配置编译及25项错误配置/符号布局拒绝检查；Flash QEMU测试目前只在
本地含交接模型的版本通过，远端CI固定的QEMU版本尚未包含该模型。

## 自主PWM配置（已烧录，冷启动未验收）

`--features pwm0-smoke`包含TCXO非阻塞检查，随后复用RAM PWM测试的配置、
占空比修改、停止和引脚恢复逻辑。Flash通过TIMER1每20ms推进一个阶段，不需要
主机发送字符：MIO21/SGPIO2/SDK47（DSLogic D11）输出10kHz的25%、50%、75%，
随后保持0%、100%，最后停止20ms并恢复引脚。原RAM测试仍保留主机逐阶段确认。

输出前通过PAC只读核对PWM时钟门控、MGPIO10/AGPIO2复用、目标GPIO方向、四组
选路、DMA、八通道使能及计数高位。条件不符打印`WS53 FAIL flash pwm handoff`
并停止，不初始化PWM或配置输出引脚。生产跨核交接仍未实现。

```sh
cargo build -Zbuild-std=core --manifest-path examples/ws53-flash/Cargo.toml \
  --release --locked --features pwm0-smoke
uv run --script scripts/test-ws53-flash-qemu.py \
  --qemu /path/to/qemu-system-riscv32 \
  --elf examples/ws53-flash/target/riscv32imfc-unknown-none-elf/release/ws53_flash_smoke \
  --test-profile tcxo-pwm0 --output /tmp/ws53-flash-pwm-new
uv run --script scripts/test-ws53-flash-pwm.py
```

模型测试以候选03运行时只读观察的M_CLKEN0=0x7bcf和PWM_DIV=7作为输入；后者
由qtest在每轮启动前设置，**不是执行了厂商FlashBoot或模拟完整上电链**。
检查六阶段实际数字波形、停止、pad释放、PWM时钟关闭及其他共享时钟位保持。
`--pwm-fault alias|group|channel|gate-off`验证接管错误被拒绝，不能将故障报告的
`passed=true`作为正常烧录资格。高位非零计数和PWM DMA行为仍不在模型支持内。

波形检查器也支持固定1MHz的DSLogic LA_CROSS_DATA（D11），使用采样时间，
不使用宿主串口到达时间。此新Flash波形已真实采集：25/50/75%阶段周期约100µs，
高电平约25/50/75µs，0%/100%保持约20.52/20.47ms。实测额外阶段开销与模型
不同，不要求周期个数逐一相同；频率/占空比、阶段顺序、保持和停止分别检查。
独立`--profile pwm04`已核对七扇区计划、
ELF和正文哈希；候选02/03入口仍拒绝此镜像。该配置同时采集D5/D11，检查
12条完整UART标记，事后只读验证PWM门控关闭、div16使能关闭及pad恢复输入。
正式CLI资格证据见`docs/plan/evidence/ws53-flash-pwm-cli-2026-10-24/summary.json`。
三次Flash实板启动证据见`docs/plan/evidence/ws53-flash-pwm-hil-2026-10-24/summary.json`；
456字节Rust串口输出与同ELF模型相同，烧录前后SWD各600次零错误。
候选04自己的七扇区恢复已通过；真正断电冷启动尚未验收。

共享源码增加后，重新构建的TCXO配置包含变化后的panic行号，正文哈希会改变。
已烧录候选03仍使用冻结的旧ELF及源码证据；没有放宽其哈希检查。默认基础配置
正文仍与候选02相同，新TCXO构建已另经两轮QEMU回归，不能冒充旧镜像实板证据。

## RTC组合配置（候选05失败定位，候选06修正）

`--features rtc-smoke`包含TCXO/PWM，并复用RAM RTC0/1的计数快照、IRQ49/50、
屏蔽/解屏蔽、周期、重装和停止测试。接管前通过PAC检查两路CONTROL和高位
装载字为0、无原始中断及本地IRQ；不停止其他客户的活动闹钟，不改公共RTC银行。
每路四次事件由普通上下文每32µs观察一次并翻转D5，随后原有TIMER0再产生32沿。
这八个RTC标记表示软件已观察到事件，不等同于精确IRQ入口时刻。

```sh
cargo build -Zbuild-std=core --manifest-path examples/ws53-flash/Cargo.toml \
  --release --locked --features rtc-smoke
uv run --script scripts/test-ws53-flash-qemu.py \
  --qemu /path/to/qemu-system-riscv32 \
  --elf examples/ws53-flash/target/riscv32imfc-unknown-none-elf/release/ws53_flash_smoke \
  --test-profile tcxo-pwm0-rtc --output /tmp/ws53-flash-rtc-new
uv run --script scripts/test-ws53-flash-rtc.py
```

正常及`--rtc-fault active0|active1|upper0|upper1`各两轮模型通过。拒绝活动客户/
非零高位装载后，客户寄存器保持；故障报告不能用于正常烧录资格。RTC周期标记
约20.515ms，目标20.5ms；D5尾部仍为32个10ms定时器沿，D11 PWM也继续检查。
每120ms计数样本独立要求3800..3900，比较UART结构时只归一化这两个值，不能
声称不同板卡的原始样本逐字节相同。

模型新增高位装载字的存储/读回/复位；非零高位计数及运行中修改仍明确拒绝，
未扩展为完整64位RTC。该寄存器此前触发未实现读取，严格测试确实失败过，
修复及回归证据见`docs/plan/evidence/ws53-flash-rtc-model-2026-10-24/summary.json`。
候选05虽然通过快速模型和整片烧录校验，真实Flash运行时却出现RTC0十二个事件。
逻辑分析仪显示周期约20.5ms，问题来自用固定次数的32µs延时拼观察窗口，额外
Flash/GPIO/MMIO执行开销使53ms窗口跨越了更多周期。慢速QEMU复现相同超计数
及下一通道断言停机；不得继续使用该镜像。`rtc05`实烧和启动观察入口已禁用，
只保留离线追溯和自身恢复。历史离线报告不代表实板通过。

候选06用独立TIMER1计数约束整个观察窗口，而非累加延时调用次数。QEMU支持
`--icount-shift 7`或`8`做指令开销压力测试，默认仍为3；该参数不模拟精确CPU频率
或缓存。新ELF在三档速度各两轮，以及四种RTC接管拒绝各两轮通过，原RAM RTC
示例也已回归。HIL配置为`--profile rtc06`，独立固定ELF、正文和七扇区哈希；
正常模型两轮、RTC样本与40沿波形、PWM及TCXO证据缺一不可。串口保留原始样本，
仅在完整结构比较时归一化两个计数值；物理复位观察也拒绝跨USB断连拼接。
候选06离线检查见`docs/plan/evidence/ws53-flash-rtc06-cli-2026-10-24/summary.json`，
修复及三档指令开销回归见`docs/plan/evidence/ws53-flash-rtc06-model-2026-10-24/summary.json`。
候选05原始失败见`docs/plan/evidence/ws53-flash-rtc05-failure-2026-10-24/summary.json`。
候选06三次Flash启动、实际波形及SWD健康检查见
`docs/plan/evidence/ws53-flash-rtc06-hil-2026-10-24/summary.json`。
自身恢复见`docs/plan/evidence/ws53-flash-rtc06-recovery-2026-10-24/summary.json`。
实际烧录/回读/恢复和冷启动以候选06自身具名报告为准，不能使用候选04/05资格。

只读观察还发现当前Flash交接的PMP配置与原RAM DMA前提不同。不能直接复用
RAM测试对已锁定、非缓存DTCM的判断；DMA Flash测试须先补齐该内存属性证明。

## USB重枚举观察配置（候选07，已通过Flash及复位启动）

`--features capture-delay`包含上述RTC组合配置。每次自主启动先实际检查
`.data/.bss`并停止厂商七秒看门狗，再只轮询TIMER1等待30秒；不使用TIMER0中断、
WFI、UART输入或主机握手。停表、计数异常或轮询预算耗尽会失败，不能把禁用
计数器读到的零值误认为已经等满30秒。正常结束后停止并清理TIMER1，再首次输出：

```text
WS53 FLASH observation delay_ms=30000 early_checks=retained
```

随后发送本次启动已完成的data/bss和看门狗检查结果，并实际运行TCXO、PWM、
RTC和TIMER测试。延迟的是首次报告与测试执行；没有重复发送旧PASS或重放旧波形。
此窗口用于给USB串口桥和分析仪重新枚举留出时间，不能补回BootROM日志或电源
最初上升沿，也不是对任意电脑USB就绪时间的保证。

```sh
cargo +nightly-2026-07-09 build -Zbuild-std=core \
  --manifest-path examples/ws53-flash/Cargo.toml --features capture-delay \
  --release --locked --offline
uv run --offline --script scripts/test-ws53-flash-qemu.py \
  --qemu "$WS53_QEMU" --elf "$WS53_DELAY_ELF" \
  --test-profile tcxo-pwm0-rtc --capture-delay \
  --output "$WS53_NEW_MODEL_DIRECTORY"
```

最终候选07 ELF为`8e78927dedb979d51f0b15bea7549e53ab0c2087296133a3e37fde1a0ad955c7`，
正文24172字节。相同ELF在`icount shift=3/7/8`各完成两轮；runner实际观察等待阶段
无UART/测试波形且看门狗已停，再检查960000000至960032000个TIMER1计数的完成
记录。它没有代替固件写入经过时间或跳过等待。`--delay-fault counter-stopped`
在模型等待阶段禁用TIMER1，两轮均保持未完成且进入panic；报告的
`expected_fault_observed=true`只表示正确拒绝，不能作为正常启动资格。
QEMU仍只建模厂商应用交接，不执行完整BootROM/FlashBoot；指令计时档位不代表
实际CPU频率。模型及主机检查见
[`ws53-flash-rtc07-model-2026-10-24`](../../docs/plan/evidence/ws53-flash-rtc07-model-2026-10-24/summary.json)。

候选07的`--profile rtc07`独立固定镜像和计划：七个正文扇区`0x5d000..0x63000`，
最后写`0x30000`组合头，共八个4KiB扇区。写前保存整片4MiB备份，写后整片回读，
保留控制核、校准数据和现场NV；恢复必须使用候选07自己的八扇区计划，不能套用
候选06的七扇区计划。正式CLI已完成实际写入、整片校验及三轮复位启动；本次共
改变23820字节，全部位于八个授权扇区，现场NV保持不变。三轮原始UART及逻辑分析仪
采样经独立离线复核，并与相同ELF的QEMU结果对照通过。具名证据见
[`ws53-flash-rtc07-hil-2026-10-24`](../../docs/plan/evidence/ws53-flash-rtc07-hil-2026-10-24/summary.json)。
候选07自身恢复和真正断电冷启动仍待验收。

成功安装候选07后，使用`hil/ws53-flash-delayed-observe.py`被动观察；参数和步骤见
[`hil/README.md`](../../hil/README.md#ws53候选07usb重枚举后的被动观察)。它允许初始USB
不在线并有限重试，各UART/分析仪会话分别落盘；一个无活动完整采集段结束后继续
新段。通过要求单个完整UART段、对应完整DSLogic段及PASS后8秒无额外输出，
不跨断连拼接；晚重连、缺标记或波形不完整会拒绝。工具不执行调试、复位、烧录或
串口写入，`cold_boot_verified`与`physical_power_cycle_confirmed`始终为false，
真实断电确认须另外归档。重连工具的主机证据见
[`ws53-flash-capture-reconnect-2026-10-24`](../../docs/plan/evidence/ws53-flash-capture-reconnect-2026-10-24/summary.json)。

## 独立 TSensor Flash 测试（软件入口，尚未烧录）

`tsensor-smoke` 只隐式启用 `tcxo-snapshot-smoke`，不默认启用 PWM、RTC、异步 TIMER
或 30 秒等待。`tsensor-restart-probe` 再增加两次明确的 START1 诊断。可分别使用：

- `tsensor-smoke`：TCXO → 温度首次转换、12 样本和重复 READY/attach → TIMER32；D5 共 34 沿。
- `tsensor-restart-probe`：上述温度阶段增加两次 START1；D5 共 38 沿。
- `tcxo-nonblocking-smoke,tsensor-smoke`：增加 TCXO 非阻塞检查，不包含 PWM/RTC。
- `rtc-smoke,tsensor-restart-probe`：TCXO/非阻塞 → PWM → RTC8 → 温度6 → TIMER32；D5 共 46 沿。
- `capture-delay,tsensor-restart-probe`：上一组合前增加自主 30 秒等待，供 USB 重枚举后采集。

首版明确拒绝 `tsensor-smoke` 与 `timer-async-smoke` 同时启用；不会生成混用 68 沿
异步协议的未定义资格。所有新配置均须使用独立模型/实板资格，旧候选07/08的冻结
ELF、烧录计划和 HIL 配置不接受温度镜像。

```sh
cargo +nightly-2026-07-09 build --locked --offline -Zbuild-std=core,alloc \
  --manifest-path examples/ws53-flash/Cargo.toml --release \
  --features capture-delay,tsensor-restart-probe \
  --target-dir target/ws53-flash-tsensor-build
```

Flash 入口自主读取 CONTROL 与 AUTO_STATUS，要求二者完整值均为零。这比仅检查
READY=0 更严格，连 READY=0 的残留 CODE 也拒绝；它是本轮资格基线，**不是硬件
复位值承诺**。同时核对控制核仍保持复位、MIE 关闭及两路 TIMER/本地 IRQ 空闲，
之后才领取传感器 token。TIMER1 提供 32MHz 有界延时；首次转换最多 10000 次轮询，
每次间隔 10µs，另有有限迭代防止计时器停止后死等。没有主机握手、传感器 IRQ、
未知 STOP、READY 清除写入、只写寄存器读回或公共时钟/电源修改。

UART 先输出 `WS53 FLASH TSENSOR BEGIN windows=N control=00000000 status=00000000`。
每个 `window=0/1/2 begin` 标记都在 D5 低电平发送，等待 FIFO 和移位寄存器排空后才
拉高；首次窗口包围初始化与首次 READY 等待，另两窗包围 START1 与新 READY 等待。
正常结束及已识别失败都先恢复低电平。D5 脉宽包含主循环配置/轮询/观察开销，不能
作为精确转换时间；START 后 READY0 的旧 CODE 只作诊断，不能当新温度样本。
共享 RAM UART 段保持原格式，成功后才有 `WS53 FLASH TSENSOR complete=1 windows=N
 gpio17_edges=2N`（实际是一行，无前导空格）；失败输出具名代码并停止，不继续后继
TIMER PASS。名义温度换算越界返回未定义标记，不钳制、不冒充校准温度。

`WS53_FLASH_TSENSOR_STATE` 保存 18 个字：phase、window、完成窗口数、边沿数、初始
CONTROL/STATUS、初始化/首次等待 ticks、轮询数、首 CODE、样本数、READY 读取数、
attach 状态、最终 CONTROL、期望窗口数、失败码、最终 STATUS、TIMER1_CONTROL 低4位。
`WS53_FLASH_TSENSOR_WINDOWS` 为三行 `[ticks, snapshot0..4]`，其中 snapshot 是
`READY<<16|CODE`，不同于原始 AUTO_STATUS 布局；首行仅最后 snapshot 有值。
`WS53_FLASH_TSENSOR_SAMPLES` 为12组 `[raw, nominal]`。内容先写、fence 后发布 phase，
仅供停住 CPU 后只读诊断，不作为 IPC。完成 phase=4；失败 phase=5；首次转换超时
代码3，未满足初始接管条件为100。TIMER1最终停止，CONTROL低4位为2。

实板只需既有 UART2、可靠 VDDIO/GND 以及 MIO17→DSLogic D5；完整组合另沿用 PWM
的 D11，不新增主动外设连接。仍须分别完成 Flash 烧录回读、复位、冷启动、恢复和
DSLogic 原始波形对照。RAM 既有温度结果不证明 Flash 继承状态正确；本入口也不证明
传感器校准、样本刷新率、温度精度、低功耗、跨核共享或 IRQ 能力。
