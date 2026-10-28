# WS53 IPC 并发存储：有条件兼容 SDK 对端

**状态：** 配套证据记录；最新分层和验收见 [实施记录](ws53-adaptation.md)。

本阶段实现生产 Rust 的并发队列存储及普通共享 RAM 后端，不再依赖暂停 CPU
交接图像。仍未确认已安装控制核的实际算法/缓存属性，没有真实跨核服务或
通知重试、drain/rearm 的验收。所有 API 继续要求 `chip-ws53,unstable`。
两个模型专用 ELF 都禁止上板；其中使用的任意测试 ID/payload 没有服务权限。

## 两个方向的不同约定

沿用12字节元数据、4字节消息头和内联≤60字节，不修改消息格式或厂商代码。
设容量 C、原始 read/write 为 R/W。应用核只写本方向的一个游标，不写 empty。

| 应用核角色 | 准备状态 | 应用核规则 | 必须核对的 SDK 对端行为 |
|---|---|---|---|
| TX生产者 | empty=1，R=W | 永远留一字节空隙，只发布 W；不清 empty | 消费者先判断R≠W，有数据就读；读完后条件写empty=1 |
| RX消费者 | empty=0，R=(H−1) mod C，W=H | H为真实消息头；释放后R增加消息长度，不写empty | 生产者按C−((W+C−R) mod C)算空间；相等且empty≠1视为满；仅见empty=1时清0 |

TX 的未读字节数 u=(W+C−R) mod C，总满足0≤u<C。新消息 L≤C−1−u 后才
复制/发布，因此 R=W 只可能为空。empty始终1，SDK消费者延迟写1不会产生
“已空却标满”或“已满却标空”；SDK必须确实先看游标，不能另加empty快路。

RX 用 H=(R+1) mod C，u=(W+C−H) mod C。如果 u<C−1，SDK看到的占用为u+1，
剩余为C−1−u；如果 u=C−1，W=R且empty=0，SDK按原规则判满。因此真实数据
最多C−1字节，最后一个字节由哨兵占用。SDK不清原本为0的empty，Rust也不写它。
消费者按H复制、验证后再发布R，使对端只复用已复制完的字节。

这是对**明确算法**的适配，不是任意固件兼容证明。两端都换为这套Rust角色时
也不能直接把TX和RX配对：约定分别针对原SDK互补角色。构造前必须让实际对端
和旧本地SDK用户静止，在厂商初始化之后准备空队列、完成所有权交接，再放行；
不能在正在运行的控制核队列上直接改R/empty。运行中禁止对端重新初始化；
游标/flag检查不能检测所有复位别名，完整复位/代次握手仍属于后续传输层。

## 内存和资源安全

`SharedMailbox: 'static` 为unsafe后端合同；`CoherentMemory`只接纳unsafe交接
的普通RAM映射，检查四字节对齐及地址溢出，不检查/修改PMP、时钟、复位或缓存。
VolatileCell保证字节/u16游标/u32容量访问宽度，RISC-V `fence rw,rw`配合编译器
屏障实现发布顺序；volatile本身不是跨核缓存一致性。两核必须是相干或无缓存
映射，不能有非相干缓存别名，也不能整行writeback覆盖对方同时修改的元数据。
SDK应用PMP源表对相关全局窗口使用属性3（无缓存、可缓冲），这是源码配置；
本阶段没有读取两核实际PMP/属性，控制核属性尚未确认，不能当成平台已交接。
可见性也不等于顺序：对端同样必须在读游标后acquire、数据访问后release自身
游标，由其屏障或已证明的硬件强顺序规则提供。Rust本地fence无法补救对端乱序；
未从SDK存在局部IRQ锁、函数返回或属性3推导这个保证，真实对端排序也未验收。

两个句柄均!Send/!Sync，本地单生产者/单消费者。`try_push`仍unsafe：远端可能
在门铃前就读到W，合法ID/长度并不授权服务、嵌入地址或buffer寿命。RX只复制/
验证内联字节，不执行回调/远端指针，错误不改游标或输出。每次操作有界，无
等待、用户回调或长临界区。Drop/forget用ManuallyDrop保留后端；仅unsafe
`reclaim_after_peer_quiescent`在证明对端/其他主机停止、引用退休并恢复内存归属
后释放。原始映射即使句柄丢失也必须保持有效，不能凭Drop复用物理内存。

## 本阶段的模型证据

6项新增主机测试使用独立转写的SDK互补算法、每次字节/标量单独加锁而非整事务
锁。四种容量65/244/1268/65536、每方向1000条，总8000条，验证满/空、绕回、
内容顺序、任意payload长度、元数据/报文损坏拒绝、Drop保留与显式回收。另在
对齐的真实宿主RAM上测试volatile后端，仍不是硅片/缓存证明。

有限SC检查对容量5/8/9/13/17的两个方向、各5个四字节空payload帧，枚举3792
状态/6814条边，逐字节验证不覆盖未读数据、不消费未发布数据、内容/游标及
终止状态。固定empty不再有两写者竞态。该有限模型不穷尽任意消息、弱内存、
cache/总线或厂商补丁，也不因通过就授予实板资格；长短帧另由主机和CPU测试覆盖。

`ipc-spsc-model`在QEMU实际执行生产HAL/volatile后端，每轮双向512条，三轮
共发送1536、接收1536。虚拟对端只在CPU**运行时**读写自己拥有的字段和空闲
数据，延迟条件empty写入；不写MMIO、CPU寄存器或结果，不为每次事务停CPU。
满243字节、跨尾、独立逐消息字节/hash、未知ID无消费/输出保持、对端显式停止
后回收及整个模型复位重启通过。这个复位测试涵盖QEMU加载器+启动路径，不独立
证明哪条BSS清零指令起效，更不代表真实控制核复位恢复或Flash冷启动。

## 控制核只读核对的失败边界

SDK `acore_memory_config.h`明确给出0x10040000，`ccore_memory_config.h`给出
本地ROM0x40000/长0x48000。固定SDK四个文件及工具身份后，AP0两次只读各
294912字节，应用核在dump阶段始终运行；健康检查阶段有正常halt/step/resume。
没有新固件、队列写入、RAM加载、复位或Flash写入。

两次返回不同：46244字节差异，出现大段重复字样。两个原始文件SHA分别为
`26bda9bcb71085f2fb4ad7b814be90753e133a297d35a86db7915a10e2c00475`、
`2adfcf403f5c77f620059488949b18c121aed53a264f2fe7265ba032914cc045`。
原脚本一致性门禁失败，保留原失败，不把该数据作为真实代码反汇编来源。
前后各600次DPIDR零错及CPU暂停/单步/恢复通过。不能从源码地址或读取成功
推断AP0返回真实ROM；也不能据此判断芯片损坏、真实ROM变动或SDK不兼容。
下一步需核对AP0/CPU数据访问、实际映射/保护及控制核Debug Module路径。

随后完成四组小块CPU/AP0对照：应用DTCM64字节、控制核启动头28字节、已知
应用ROM128字节、控制核ROM前128字节，均为CPU两次稳定且AP0相同；应用ROM
同时匹配旧参考。强制重读的PC/SP/FP/S1前后相同，未触发寄存器修复，恢复运行。
第一次尝试因OpenOCD仅接受fp、拒绝s0别名，在读数据前退出并恢复，原失败保留。
两次对照各前后1200次DPIDR零错，连同原大窗口读取共3600次及三组CPU调试恢复
通过。没有镜像、队列写入或请求MCU复位。四个小样本一致不证明整个窗口是真实
代码/全只读：旧整窗的72个4KiB页中71个只有至多两种u32值，变化仍未解释，
没有把稳定重复数据解码成真实算法。原大窗口门禁继续失败，不由小样本覆盖。

SDK跟踪的184320字节control_ws53.bin SHA为
`fe094af56207c742249f5fb34feb111779b8599a11c57f056e2708c05a299a41`；
未找到应用ROM那几段精确字节，只说明本次精确搜索未匹配，不能证明算法不同
或缺少IPC。原始固件/读出文件都保留在私有hardware目录，不进入公开归档。
当前启动头前28字节整体未匹配该SDK blob前28字节，只记录逐字节比较结果；
保留/运行时字段也可能变化，不能据此判定整个控制核固件版本或算法不同。

### 后续：原厂 Flash 镜像与 AP2 状态核对

`scripts/ws53-control-image-compare.py`在固定SDK提交及六个原始文件的前提下，
核对原厂4MiB备份身份、已验收的明文组合头profile、body边界和SHA。组合头
为0x300字节，控制核有效比较区为物理Flash `[0x30300,0x5d000)`，不是把
CCore ROM startup的0x340当作Flash头。SDK blob长0x2d000，比较仅覆盖两者
共同的0x2cd00字节；剩下768字节不用于本次比较，也不据此诊断镜像越界。

该区有175527字节不同。原生读取的28字节启动头与原厂备份该区开头完全相同，
与SDK blob不同。这次已经有真实Flash内容比较，能够确认两个比较前缀不相同；
仍未识别版本号、编译配置、控制核ROM或IPC算法，不能把“不相同”写成“不兼容”。
原始Flash/启动头不公开，只保存校验值。十项篡改/边界反例被拒绝，另确认
同长度但不同的启动头只报告不匹配，不冒充固件身份验证。

`hil/ws53-debug-modules-readonly.py`只通过现有mem_ap读取AP1/AP2的DMCONTROL、
DMSTATUS、HARTINFO、ABSTRACTCS各两次，采用源码核对的DMI地址左移2。
AP1两次均为`1/c82/0/03000001`；AP2为`0/c0c82/0/03000001`。
两者均解码version=2、authenticated=1、datacount=1、progbufsize=3。
AP2的dmactive=0，本次未写DMCONTROL/COMMAND或激活、暂停控制核；其原始
running/复位应答位不能独立证明当前控制核执行状态，也不证明AP2的CPU归属。
应用核标准初始化和前后健康检查仍包含已有vendor debug-enable及应用核
halt/step/resume；“只读”限定新增快照动作，不把整套探针操作称为无副作用。

前后共1200次DPIDR零错、应用核正常继续；未加载新程序、请求MCU复位或改写
Flash。候选19此前已通过独立nRESET、hisiflash同页恢复、整片回读和原厂AT，
当前继续保留原厂程序。用户暂不便断电，冷启动仍未验收。下一步需为AP2建立
可恢复的控制核专有读取路径，并核对真实对端排序/缓存/交接，不能直接将
model-handoff ELF上板。

### 实测 SRAM 布局和类型化描述符

隔离的AP2 CPU进程完成RV32读取（misa=0x40901104，应用核为0x40901124），
读取前后PC及31个通用寄存器相同，恢复运行后关闭本轮临时激活的AP2 DM，
原厂AT正常。第二轮增加CPU/AP0对照，启动头28字节、startup256字节、
text256字节三组相同；ROM128字节对照失败，整轮明确保持失败。这些是电脑
重启前的记录，不作为重启后的连接状态。原始寄存器和数据保留在私有目录。
隔离进程的理由是当前OpenOCD的get_dm仅按TAP查找缓存，未区分不同AP；
本阶段没有修复或宣称同一进程双CPU目标可用。

用户随后确认电脑重启。防自动更新库由仓库C源码重建，SHA与旧冻结工具相同；
固定Python依赖缓存改放工作区target，重新核对探针23121302、串口5CB7019921。
重启后第一份SRAM采集因旧脚本要求AP2 dmactive=0而失败，保留producer和日志。
实际前后均为1；不把该状态归因于某个复位或软件，也没有为了通过而改写DM。
新脚本要求显式--expected-ap2-dmcontrol，在采集前检查状态，并在结束时确认
完全保持；不符立即拒绝。十个Tcl场景及缺失/重复字段两项反例已验证，均无USB。

新一轮`ccore-sram-read-02`通过，AP2 DMCONTROL前后1、DMSTATUS前后0xc82。
采集期间不请求CPU暂停，前后健康检查仍正常暂停/单步/继续应用核。两份
`[0x10010000,0x1001ff00)`的65280字节完全相同，SHA均为
`9363caa1655df65ce9e0b4c2fb6165f71f83c63a61b2dd12dd9064c195334afe`。
采样相同只表示这两次文件字节一致，不证明缓存一致性或整个窗口为指令。
按32字节唯一非低熵片段与原厂Flash控制区做精确比较，共匹配35168字节，
仍不等于函数/算法识别。三段应用ROM IPC尾部的精确字节未匹配，不能据此
判断控制核缺少IPC。重启后两轮健康检查合计2400次DPIDR零错，AT前后正常。

启动头word6指向控制核本地0x1e300，应用核视图0x1001e300，实测元数据如下：

| 应用方向 | 控制核地址 | 应用核地址 | src/dst/ch | 总长/数据容量 | read=write | empty |
|---|---|---|---|---|---|---|
| RX0 | 0x1e300 | 0x1001e300 | 0/2/0 | 1280/1268 | 1236 | 1 |
| RX1 | 0x1e800 | 0x1001e800 | 0/2/1 | 256/244 | 24 | 1 |
| TX0 | 0x1e900 | 0x1001e900 | 2/0/0 | 1280/1268 | 968 | 1 |
| TX1 | 0x1ee00 | 0x1001ee00 | 2/0/1 | 256/244 | 12 | 1 |

布局与固定SDK应用初始化相符。新增`ipc::layout::{ControlSramRange,
ControlLocalAddress,ApplicationViewAddress,SdkMailboxLayout}`，从复制的28字节
启动头推导四条队列及typed Route。必须显式提供已确认SRAM范围；检查对齐、
溢出、范围和128字节启动头重叠，禁止把应用核地址再次当成本地地址换算。
这套安全API仅解析普通字节，不读MMIO/共享RAM，不创建引用或授予所有权。
SDK最大SRAM边界只是数值上限，不默认选择device/MCU模式或假设全部bank存在。

实测四组公开协议元数据进入主机回归；包含async/Embassy/defmt共415项测试
通过，另有RISC-V Clippy和18组芯片feature检查。模型专用Rust程序执行同一
布局解析、四条路由和五项拒绝，三轮双向各1536条存储事务继续通过。布局测试
使用复制的普通字节，没有在QEMU伪装成真实控制核握手。两份新构建完整ELF
同SHA `d7052c6c28f911d5c8bfb32c58e1e10cfcc55692c70181ceee531a94561aad73`。
本阶段未向实板加载这份ELF，也未请求MCU复位、Flash、eFuse或安全配置写入。
真实IPC的对端算法、排序/缓存、交接和通知/服务仍未验收；电脑重启不算冷启动。

### AP0访问属性对照：bit24不是已测ROM窗口的解决办法

固定SDK objdump与已采集SRAM、原厂Flash的32字节唯一上下文，核对三条标准
JAL候选：0x14d3c→0x5a956、0x14db2→0x406ea、0x16aea→0x6b7f8。
独立解码JAL立即数与厂商反汇编一致，源字节与原厂控制区匹配；这只是编码和
候选目标的证据，没有在这些PC上抓到执行，也没有识别出IPC收发函数或原型。
不将原始镜像作为公开附件。

[Arm ADIv5 IHI0031G E1.5/Table E1-7](https://documentation-service.arm.com/static/622222b2e6f58973271ebc21)
把AHB3 CSW bit24映射到Data属性，其支持情况由实现决定，未实现时固定读1。
新增`hil/ws53-ap0-csw-read.py`，仅比较厂商默认0x43800000和清bit24的
0x42800000，其他字段保持，结束强制恢复默认并通过已知ROM读推动实际AP设置。
不改变安全状态位、PMP、固件或AP2控制。五个合成Tcl场景检验正常/早期拒绝/
各阶段读失败时恢复CSW，不能当作硬件验证。

第一轮硬件已完成读回与恢复、SWD/AT正常，但脚本错误地把`dap apreg`的
command_print输出当成Tcl返回值，因此解析失败。保留原producer和报告。
根据本地OpenOCD实现改为`capture`，用无接口、无init的version命令独立验证
捕获API，再修正合成命令为真实的打印/空返回行为，在新目录完成第二轮实测。

实测CSW为0x43800062→0x42800062→0x43800062，证明bit24读回可变且恢复。
应用ROM0x1f220的128字节、控制核头0x10010000的28字节在所有profile下匹配
既有参考。控制核0x10040000、0x100406e0、0x1006b7f0各128字节，每种profile
两次稳定且彼此相同，但每块仅两种u32值，仍不能作为真实代码使用。这只说明
清除此位未改善三个测点，不诊断为ROM损坏、加密或某种确定的保护机制。
两轮前后2400次DPIDR零错、原厂AT正常；未加载镜像、写Flash/队列或请求复位。

另核对SDK的`sec_trng2_updata_handler`：IPC18是控制核请求应用核向共享区
填充随机数，不能直接当成应用核请求控制核的ping/随机服务。原回调忽略
payload_len并直接使用三字请求中的地址/长度；Rust适配仍须先校验12字节
载荷、更新类型、地址/长度溢出、独立缓冲授权和两核排序，再接入已有TRNG
驱动。CT_SRAM队列布局类型不自动代表任意复用RAM随机缓冲的可用范围。
本阶段没有新增该服务的实现或验收，也没有因此放宽原并发后端的unsafe合同。

### IPC18 随机缓冲图像服务（2026-10-28）

新增`ipc::random_buffer`，提供`BufferRegion`、`UpdateRequest`及
`RandomBufferImage`。按SDK三字小端请求检查ID18、恰好12字节、更新类型、
本地结束地址和应用核别名结束地址溢出。登记范围由平台显式提供，报文中的
整块起点/长度必须完全匹配；不能拿报文自身地址重新“登记”后跳过授权。
这是普通独占字节图像的描述，不把任意数值当成已存在/已供电的RAM，也不把
CT_SRAM队列范围误用于控制核可能在复用RAM堆中分配的随机缓冲。

类型0更新整块，1/2更新前/后半块。半块要求整个长度为偶数；整块支持尾字节，
每个尾部取新TRNG字，不复制SDK回调中可能重复上一字的残余字节。
`RandomBufferImage::update`只接收已校验请求，复查其登记范围、暂存容量，
调用已有`TrngDriver::fill_bytes`，使用一个全调用总尝试预算。私有暂存区完整
填充后才复制到选定范围；源关闭、超时或取数部分成功均不修改目标。错误中的
written表示暂存前缀，熵消耗不能回滚；暂存含随机数据，不提供秘密擦除保证。
复制期间不持有全局临界区，不分配堆内存，不发ACK/通知，也不自动释放peer。

仅对普通独占图像开放此安全API。不能从接收地址直接构造`&mut [u8]`，
不能向仍由控制核读取的共享区创建该引用。真实适配还需单独取得整个缓冲的
生命周期/独占lease、映射与缓存所有权，确认成功/失败完成协议及服务重入。
因此本节不是“实际控制核随机服务已完成”，也没有放宽SPSC后端合同。

新增模型专用`ipc-random-model`，仍强制`model-handoff`，严禁上板。
运行中的host虚拟peer分段发布SDK格式帧，真实Rust接收后经生产HAL/PAC取TRNG。
登记地址0xc1000是协议元数据，图像放在应用模型RAM；没有伪装成对该控制核
物理地址的写入。host只在上条完成与下条W发布之间更改测试输入/图像；
CPU执行填充时host不碰目标。TRNG时钟/环/FIFO和错误注入由host显式设置，
模型为确定性xorshift源，既不证明熵质量，也不证明真实跨核交接。

复现入口（父仓库根目录，所有路径均支持相对输入）：

```sh
uv run --script scripts/ws53_ipc_random_build.py --output target/ipc-random-elf
uv run --script scripts/test-ws53-ipc-random-model.py \
  --qemu /path/to/source-bound/qemu-system-riscv32 \
  --model-receipt /path/to/source-bound/receipt.json \
  --model-source /path/to/hisi-riscv-qemu-ipc-mask \
  --elf target/ipc-random-elf/firmware.elf \
  --receipt target/ipc-random-elf/receipt.json --output target/ipc-random-run
```

测试包括两个种子、整块/两半及非整字尾部、各类请求拒绝、暂存不足、总预算
只接受一个字后的失败、环/时钟关闭、七种FIFO错误、各失败后的成功重试和
每种子末尾的模型复位恢复。host独立计算字节及FIFO消耗，错误时要求目标/守卫
完全不变；解析或暂存拒绝要求FIFO/status访问均为零。具体具名结果见适配计划，
CI同一qemu-ipc任务加入该程序的两次全新构建比较和集成场景。

### 独占图像服务的受控RAM实测

`examples/ws53/ram_smoke`新增`ipc-random-smoke`，复用原TRNG接管和恢复流程，
在本地私有图像执行13项检查并记录完整before/after/scratch。A板三轮39项与
同ELF QEMU13项通过，每轮物理复位恢复原厂AT，前后1200次DPIDR零错。
完整新ELF为`c3fda1237489a8c4d62d5df9f30b4dcbaaa5e918b659bab4895a700a63e75d03`。
主机重新逐字节验证实际报告，预算耗尽三轮均只改变私有暂存的4字节、目标不变。
这没有执行真实控制核回调，登记地址仍为元数据，没有live buffer lease。

构建和模型复现（父仓库根目录；QEMU需使用已有来源收据对应的固定构建）：

```sh
uv run --script scripts/ws53_ipc_random_ram_build.py --output target/ipc18-ram
uv run --script scripts/test-ws53-uart-host.py --qemu "$WS53_QEMU" \
  --elf target/ipc18-ram/firmware.elf --mode trng --output target/ipc18-ram-qemu
```

HIL入口须另外传入`--ipc-random --ipc-random-receipt target/ipc18-ram/receipt.json`
及新鲜固定设备健康报告，完整边界见[HIL说明](../../hil/README.md)。
对照脚本`compare-ws53-ipc-random-ram.py`要求当前构建/模型收据以及逐轮原始
记录；随机值不要求一致，比较选择范围/状态/目标和暂存不变量。
真实数据/串口raw留在私有目录，仅公开结果和哈希。

实验仍观察到两个SDMA门控位无法清零，不证明物理排空或生产跨核资源交接。
内部TRNG不驱动可测外部数据线，本项没有新增逻辑分析仪波形或熵质量证明。
Flash、冷启动、初始化、真实控制核服务/缓存/通知交接继续未验收。
具名报告及限制见[适配记录](ws53-adaptation.md#2026-10-28ipc18-图像服务首次受控-ram-真机及同-elf-对照)。

### SDK ring启动profile（上游时钟/源仍须交接）

在上述私有图像样例基础上，新profile显式测试生产HAL
`TrngDriver::start_sdk_ring`的0→1与1→1路径。固定SDK应用只启用ring分支；
该API不配置可选FRO/CLK_OPEN、上游电源/复位/时钟，也不证明熵质量。
只改变16位ring字的bit0，等待至少200us并核对整个字；失败保留token、不给出
FIFO数据、不自动回滚。`from_configured`的继承语义及release/Drop保持不变。

```sh
uv run --script scripts/ws53_trng_ring_build.py --output target/trng-ring-ram
uv run --script scripts/test-ws53-uart-host.py --qemu "$WS53_QEMU" \
  --elf target/trng-ring-ram/firmware.elf --mode trng --output target/trng-ring-qemu
```

该ELF的QEMU运行不由host开启ring，握手前要求完整字0。HIL必须选择
`--trng-ring-start --ipc-random-receipt target/trng-ring-ram/receipt.json`，另外
传入固定工具、新鲜健康报告及受控观察参数。确认原字1后，host按SDK已知16位
操作关到0，Rust必须重新打开；这不是断电重启，也不清除已有源/时钟配置。
新schema与普通图像profile分开，二者不能交换收据/硬件flag。

同ELF三轮六次启动及后续39项图像、逐轮原厂恢复通过；名义32MHz TIMER下
首次6518/6518/6517、已开启6515ticks，QEMU均6467，达到6400ticks下限。
没有新增物理频率校准或外部波形。使用对照脚本时另加`--start-ring`，它会
同时复核启动记录及普通图像记录。完整源初始化、SDMA排空/生产交接、
真实控制核共享服务、Flash、冷启动和熵健康检测继续待做，不能毕业为稳定API。

### 独立Flash图像样例（候选20，2026-10-28）

`examples/ws53-trng-flash`通过厂商FlashBoot进入Rust，不由调试器加载应用RAM
或初始化时钟。startup重建data/BSS，控制CPU保持复位；显式释放已核对的控制域
logic/clock，不调用涉及根密钥更新的整套SDK加密环境初始化。
SDK ring两次等待/完整字核对后，在私有普通RAM图像上执行既有13项检查。
这没有启动实际控制核或向其发送完成通知。

hisiflash真实烧录及独立nRESET两轮共4次等待、26项图像通过。两次实板入口
ring均为1；QEMU普通启动从0开始且host不写时钟/ring，另覆盖脏RAM初始化、
源时钟故障及复位恢复。比较功能合同和错误路径，不要求随机字节或模型时延等同。
D5=MGPIO17三脉冲由4MHz逻辑分析仪采集，覆盖两次启动和一轮图像检查，
与TIMER名义32MHz记录在规定容差内一致；第一轮为344.5us/348.5us/16.4835ms，
独立复位为344.75us/349us/16.5ms。TIMER记录还含标记/读取开销，不能把窗口
视为TRNG内部信号或精确时钟/熵证明。结束恢复引脚、关闭UART中断和定时器，
RX FIFO为空。旧观察器把0x4c的RX计数误标为DMA；该项不能证明DMA关闭或排空。

构建与验收入口为`scripts/ws53_trng20_build.py`、`scripts/test-ws53-trng20.py`、
`scripts/test-ws53-trng20-qemu.py`和`hil/ws53-flash-trng20-cli.py`，
独立复位为`hil/ws53-flash-trng20-srst.py`。原始随机字节/厂商固件/波形私有保存。
真正断电冷启动、完整源/时钟、SDMA排空、live lease/缓存及真实完成协议仍未验收。
测试后hisiflash显式恢复原厂，整片回读等于烧录前备份、当前NV不变、AT通过；
前后四轮2400次DPIDR零错，CPU调试正常。A板当前运行原厂应用。

## 构建、发布与未完成项

新增SPSC独立bin/schema构建器，和原门铃构建器隔离，交叉profile收据必须拒绝。
vcell使用现有0.1.3，仅chip-ws53激活；其他芯片接口不变。17个WS53 workspace
采用完整Cargo依赖解析更新锁文件并重查--locked，不使用--no-deps伪装验证。
HAL独立开发锁通过显式本地PAC/运行时覆盖生成，不写入机器路径。原注册表离线
检查因缺ws53-pac缓存未通过；本地覆盖不是注册表可用/发布验证。发布和外部
消费者纯注册表构建还需依赖提交/版本可用后另验，不据此宣称已发布。

实时存储模型继续推进了P5，但真实控制核算法、两核缓存/归属、服务buffer授权、
通知drain/rearm/丢唤醒和低功耗仍未完成。独占图像的受控RAM及Flash正常/nRESET
子路径已有具名实测；真实跨核门铃/实时存储、真正断电冷启动仍未验收。
外部标记窗口不替代内部随机源或真实IPC电气验证，完整P0–P7继续。
