# WS53 I2C 数字线序与实板验证

**状态：** 配套证据记录；最新验收状态以 [实施记录](ws53-adaptation.md) 为准。

状态：实验实现。两路RAM地址NACK及恢复已实测，正常收发只完成虚拟器件验证。
首轮候选22的TIMER/标记配对失败原样保留。新版四次边界采样已通过RAM、QEMU、
hisiflash实际烧录/整片回读及独立物理nRESET，随后精确恢复原厂4MiB和当前NV；
断电冷启动、外部器件、DMA及模拟时序仍待验收。

候选22首轮的170项失败/恢复证据封存在
[独立档案](evidence/ws53-i2c22-first-flash-2026-10-28/README.md)。Flash执行的
GPIO边沿外开销约40µs，超出旧经验容差，原失败保持不变。新版在置高前后、
置低前后分别读取T0/T1/T2/T3，逻辑分析仪窗口必须位于内外计时区间之间，
仅加入仪器和计时器量化预算；报告版本`0x53f02202`明确拒绝旧版。
两路新版RAM各一轮、五场景Flash QEMU、真实Flash及独立nRESET均已分别验证。
两次Flash运行共28次地址NACK/STOP、4个空事务窗口，实际AP0、UART与DSLogic
一致，来源前后未变；恢复交易完整镜像精确等于测试前现场备份，原厂AT正常。
未通过调试器加载RAM或预设外设时钟；FlashBoot继承源仍不等于完整时钟树初始化。
请求100kHz实测约88.15/87.67kHz，频率差异未解决。当前进度见
[适配记录](ws53-adaptation.md)。

[新版Flash/复位证据](evidence/ws53-i2c22-flash-2026-10-28/README.md)共175项，
包含独立的模型、RAM、Flash、自主复位及精确原厂恢复记录。新版六次健康检查
共3600次DPIDR零错，最终A板运行原厂应用；没有进行真正断电冷启动。

## 已确认的范围

SDK依据为`fbb_ws53`提交`163d9150ab94a02d09a05f623351f9facee3025e`的
`src/drivers/chips/ws53/porting/{i2c,pinctrl}`。I2C0/1对应IRQ62/63、
M_CLKEN1.bit6/7及32MHz参考源。只操作已确认的门位，不套用其他UART的冲突枚举。

| 实例 | 本轮SCL | 本轮SDA | 分析仪 | RAM验收 |
|---|---|---|---|---|
| I2C0 | SDK47 / SGPIO2 / MGPIO21，mode3 | MGPIO22，mode3 | D11 / D18 | 三轮各七次地址NACK、recover、空事务及恢复 |
| I2C1 | MGPIO8，mode3 | MGPIO9，mode3 | D15 / D14 | 同上 |

MGPIO17/D5标记八个事务窗口。SGPIO0/1是SWD别名，本轮不修改或测量。
控制核保持复位，其他CPU/DMA资源只按既有受控RAM接管流程隔离，不宣称已经实现
生产环境跨核交接或物理SDMA排空。每轮完成后通过已验证的整板复位恢复原厂AT，
不恢复已经被RAM镜像覆盖的旧CPU上下文。

逻辑分析仪4MHz、阈值1.65V，先捕获再加载RAM。10/100kHz请求下的7位写、7位读
和10位尝试均记录错误源/细节、读缓冲哨兵及TIMER；七次地址首字节分别为
0xAA/0xAB/0xF4，后面NACK和STOP。10位地址未被完整发送，不能称完整10位收发。
空事务两线持续高。外部只有分析仪输入，使用内部上拉，没有确认合适的外部上拉电阻。

## 模型及对照

隔离模型提交`6d4c0699048c48a037ccdbcd228b433df981b997`，
父工程策略`scripts/ws53_ci_qemu_i2c_wire_build.py`固定55个源/补丁/构建输入。
该提交目前仅在本地，远端CI尚未接入这个模型增量，不能声称远端CI通过。

默认保留原I2C寄存器/FIFO模型。新增模型需要同时显式启用：

```text
ws53-i2c.assume-profile=on
ws53-i2c.assume-wire-profile=on
ws53-pinctrl.assume-i2c-wire=on
```

无外部器件场景还设置`ws53-i2c.wire-peer-connected=off`。正常场景使用原有
7位0x50、10位0x250的独立易失寄存器对端。地址与数据决定实际线序，未预设
一段“通过”波形。所有1位均释放总线，只有0位主动拉低；缺少唯一SCL/SDA、
输入缓冲或内部上拉时，数据进度停止。多输入没有假定优先级。

非STOP读先提供八位数据，保持SCL低，等下一命令决定第九位ACK/NACK；
读转写先NACK，再重复START。十位初始读发送两个地址字节并重复START；
高位前缀匹配但第二地址字节不匹配时，第二字节NACK并上报detail.bit2。
原型误将此情况当作首地址NACK，最终模型已经修正。

13组SDK路由组合通过缺路由/上拉/输入、延迟ACK、读转写、连续读、门控暂停、
重复选路阻塞、10位第二字节NACK及复位恢复测试。真实Rust正常收发程序共三组
exercise、57段数字事务，独立核对每个字节、ACK、NACK、重复START和STOP。
这些正常事务只有虚拟对端证据，尚无正常外部器件实测。

同ELF实板与模型比较了逐阶段操作/错误结果、地址首字节、应答、时钟边沿数和
恢复状态。每侧D5窗口分别对照自己的TIMER，不要求跨平台CPU/MMIO执行时间相等。

| 请求 | 实板频率 | 数字模型频率 | 判定 |
|---|---|---|---|
| 10kHz | 约9.86kHz | 约9.997kHz | 协议子集一致，频率有差异 |
| 100kHz | 约87.7–88.4kHz | 约99.68kHz | 频率差异未解决 |

模型没有模拟RC上升时间、输入滤波、模拟供电或驱动强度。不能通过调模型延迟或
直接补偿HAL分频使测试“对齐”；需后续模拟波形和外部上拉对照，确定真实原因。
`stretch`既有注入仍是功能暂停，不是多器件物理拉伸时序的证明。

## 复现数字测试

在父工程根目录执行，模型源工作树需要上述本地提交。构建普通Rust程序不需要SDK，
以下模型构建需要QEMU依赖；安装过程沿用模型仓库`scripts/setup-deps.sh`。

```bash
qemu_python=$(uv run --python 3.12 --no-project -- sh -c 'command -v python')
uv run scripts/ws53_ci_qemu_i2c_wire_build.py \
  --overlay-root ../hisi-riscv-qemu-ws53-i2c-wire \
  --python "$qemu_python" --output target/my-i2c-model --jobs 2
uv run scripts/test-ws53-i2c-wire-mmio.py \
  --qemu target/my-i2c-model/qemu/build/qemu-system-riscv32 \
  --model-receipt target/my-i2c-model/receipt.json \
  --model-source ../hisi-riscv-qemu-ws53-i2c-wire --output target/my-i2c-mmio
uv run scripts/ws53_i2c_traffic_build.py --output target/my-i2c-traffic
uv run scripts/test-ws53-i2c-wire-traffic.py \
  --qemu target/my-i2c-model/qemu/build/qemu-system-riscv32 \
  --model-receipt target/my-i2c-model/receipt.json \
  --model-source ../hisi-riscv-qemu-ws53-i2c-wire \
  --elf target/my-i2c-traffic/firmware.elf \
  --receipt target/my-i2c-traffic/receipt.json --output target/my-i2c-normal
```

两路无器件镜像分别用`ws53_i2c_wire_build.py --bus 0/1`生成，
用`test-ws53-i2c-wire-qemu.py`接收同样的模型参数及对应ELF/receipt。
同一输出目录不可复用；每个新构建、测试使用新目录，保留失败日志。

实板入口`hil/ws53-i2c-wire-capture.py`要求新鲜的`ws53-debug-health.py`报告、
来源匹配的专用ELF及receipt；专用镜像拒绝混合其他功能features。
只在设备身份、内存归属及接线符合当前受控A板配置时运行。此入口加载RAM并在
finally路径复位恢复；它不擦写Flash，不是Flash/cold-boot验收入口。

## 下一步

先用示波器及有明确外部上拉的总线查明频率差异，再用真实器件/第二板对端验证
正常数据、完整10位寻址、重复START、超时和恢复。I2C Flash自主启动、断电冷启动、
外设DMA和async仍分别保留验收项。现有数字模型不能代替这些结果。
