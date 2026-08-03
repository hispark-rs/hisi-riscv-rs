# HIL 脚本与 runner 环境变量

HIL（hardware-in-the-loop）脚本和 cargo runner 的环境变量参考。事实取自
`hil/embedded-test-runner.sh`、`hil/hil-smoke.sh`、`hil/flash.sh`、`hil/pack.sh`、`hil/cargo-run-hw.sh`
以及 CI/agent wrapper `.agents/skills/hil-smoke/hil.sh`。
完整示例清单、成功标记串和失败标记串的唯一事实源是 [示例目录与验证标记串](02-examples.md)。

HIL 框架原理见 [HIL 测试框架](../explanation/07-hil-framework.md)；运行步骤见
[运行 HIL 测试](../how-to/07-run-hil-tests.md)。

## HIL 入口速查

| 入口 | 用途 | 观测通道 |
|------|------|----------|
| `hil/embedded-test-runner.sh` | `hisi-hal --test hil` 与 `tests-hil` 的 on-target test runner | `probe-rs run` + RISC-V semihosting，libtest 兼容输出 |
| `hil/hil-smoke.sh` | WS63 示例级 UART smoke | UART0 grep 标记串 |
| `hil/ws63-connectivity-smoke.sh` | WS63 A4/W2 connectivity gate | upstream profile: plain Cargo RF link；vendor oracle: guarded link；之后均为 planned-bin download + J-Link nRST + UART contract |
| `hil/ws63-a5b-response-bound.sh` | A5B + connectivity final-image gate | 公开 `wifi_connectivity`：一次 verified download + 20 次 unchanged-image J-Link nRST + 完整网络 contract + 100 ms fail-closed runner bound |
| `.agents/skills/hil-smoke/hil.sh` | CI/agent wrapper：preflight、chip 封装；WS63 全套委托 `hil/hil-smoke.sh` | UART0 grep 标记串 |
| `hil/flash.sh` | 示例/固件烧录封装 | hisi-fwpkg plan + probe-rs bin download，或 hisiflash |
| `hil/cargo-run-hw.sh` | 把单次 `cargo run` 改成烧真机 | hisi-fwpkg plan + probe-rs bin download，可选 UART stream |

## 串口约定

| 串口 | 用途 | 参数 |
|------|------|------|
| UART0 = `/dev/ttyUSB0` | 示例输出 / `hil-smoke.sh` grep | 115200 8N1 |
| `ttyACM0` | J-Link VCOM | - |

## `embedded-test-runner.sh`

Cargo 以 `<runner> <built-test-elf> [embedded-test args...]` 调用 runner。脚本先原地执行
`hisi-fwpkg patch-hash <elf>`，再调用补丁版 `probe-rs run`；尾随参数会原样转发给 `embedded-test`
测试调度。

| 变量 | 默认 | 说明 |
|------|------|------|
| `PROBE_RS` | `probe-rs` | probe-rs 二进制；需要 `hispark-rs/probe-rs` 的 `add-hisilicon-ws63-bs21-hil-baseline` 分支 |
| `PROBE_CHIP` | `WS63` | probe-rs `--chip` 值 |
| `PROBE_YAML` | 空 | `--chip-description-path` YAML；需要显式指定时填 `HiSilicon_WS63.yaml` |
| `PROBE_SPEED` | `2000` | 调试传输时钟，单位 kHz |
| `HISI_FWPKG` | `hisi-fwpkg` | `hisi-fwpkg` 二进制名，用于 `patch-hash` |

典型启用方式：

```bash
PROBE_YAML=/path/HiSilicon_WS63.yaml \
CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/embedded-test-runner.sh \
cargo test -Zbuild-std=core,alloc -p hisi-hal --no-default-features --features chip-ws63,rt \
    --target riscv32imfc-unknown-none-elf --test hil
```

## `hil-smoke.sh` 当前检查的 grep 模式

`hil-smoke.sh` 逐例烧录后读 UART，用 `grep -qE` 匹配下列模式（`check <example> <egrep> <desc>`）。
这是脚本行为参考，不是完整示例标记串清单；完整清单见 [示例目录与验证标记串](02-examples.md)。

| 示例 | 匹配的 egrep 模式 | 描述 |
|------|-------------------|------|
| `uart_hello` | `Hello from WS63` | 已在 WS63 真机通过；验证 HAL UART boot-clock 配置与 115200 8N1 输出 |
| `timer_irq` | `timer irq #|OK: timer` | Timer IRQ 投递（验证 24 MHz TCXO 定时器时钟） |
| `gpio_irq` | `gpio irq #` | GPIO IRQ 投递 |
| `reset_demo` | `OK: software reset observed` | software_reset + reset_reason（第二次启动标记） |
| `spi_loopback` | `SPI loopback OK` | 阻塞 SPI0（先短接 MOSI<->MISO） |
| `i2c_scan` | `scan done|no devices` | I2C0 扫描 |

`blinky`（GPIO 翻转无 UART，需 LED/逻辑分析仪）与 `semihost_selftest`（需 debugger 半主机）在裸 HIL 跳过。
总结果：全过打印 `HIL SMOKE: PASS`，否则 `HIL SMOKE: FAIL` 并 `exit 1`。

## A4 connectivity marker contract

`hil/ws63-connectivity-reset-matrix.py` 是连接性 marker/diagnostic contract 的唯一可执行
事实源；单次 `hil/ws63-connectivity-smoke.sh`、多次 unchanged-image nRST 和离线复算都调用
同一 classifier。smoke 脚本与普通
外设 smoke 分开，因为需要受控 Personal-mode AP、secret 和更长的 UART 窗口。
完整 `upstream-wpa2`/`upstream-wpa3` profile 构建公开 `wifi_connectivity` 示例。该示例
只直接依赖 `hisi-rf`，使用 Cargo-delivered normalized archives，执行普通
`cargo build --release`、final-ELF gate、`hisi-fwpkg plan`、probe-rs bin download 和
J-Link nRST；不调用外部 RISC-V GCC 或 post-link patch。credential-free init/scan 和
crypto contention 仍使用 `wifi_init_smoke` maintainer fixture。`vendor-wpa2` 仍从公开
`ws63-RF` release 下载固定 archive（或接受 `WS63_WPA_ARCHIVE`），校验 SHA-256 后走
guarded link，但该分支只作为迁移 oracle。

严格契约 `ws63-connectivity-markers/v2` 按顺序要求
image/init/scan/connect/DHCP/local-data-path/summary/steady/renew
阶段及对应 `A4_RADIO_EVENT`；DHCP 之后还必须给出公网路由处置 marker。缺失或乱序
marker、fatal marker、非零内部 queue drop / TX error / backend error、A5B runner budget
越界都会失败。本地硬门槛是 DHCP 成功，并通过 UDP 发送触发 neighbor discovery，要求
`RFDBG_A5B_L2` 记录 ARP reply；目标优先使用 DHCP router，没有 router 时使用 DHCP server。
有默认路由的 profile 还必须向 AliDNS `223.5.5.5:53` 与 Baidu DNS
`180.76.76.76:53` 交替发送固定 A 查询，至少一项返回来源、transaction id、QR/opcode、
TC、rcode、question 和 answer count 均合法的响应。隔离 SoftAP 没有下发默认路由时，
必须输出精确的 `RF5C_PUBLIC_DNS_SKIP reason=no-default-route`，同时 DNS 尝试/响应计数必须
为零；这只跳过不可路由的公网探针，不放宽 DHCP、本地 ARP、summary、steady 或续租门槛。
`RF5C_PUBLIC_DNS_ERR` 单独分类为 `public_dns_failure`，不与
`local_data_path_failure` 或 association failure 混合。v8
`RFDBG_A5B_L2` 另行记录双向 ARP request/reply、IPv4 与 other frame 计数，供失败
归因。reset-matrix summary 会汇总
每个 L2 计数器在带 marker 轮次中的最小/最大值，并单列缺失 marker 的轮次。构建后先冻结
`hisi-connectivity-artifact/v1`（profile、ELF SHA-256、marker contract），解析前再次复核；
不一致时拒绝把 UART 结果归到该镜像。成功或失败都把 raw UART、identity 和 summary 留在
`EVIDENCE_DIR`，含凭据的临时 ELF/target 仍在退出时删除。

普通 CI 的 QEMU target fixture 也执行同一 parser，但必须携带
`RFDBG_CONNECTIVITY_CONTRACT_FIXTURE scope=contract-only`。它只证明目标端 marker
producer、artifact identity 和 classifier 契约可执行，不能替代真实 RF、AP 或硅片 HIL；
该 marker 若出现在真机 capture 中反而会 fail closed。

A5B 上界不能靠事后肉眼看最大值。专用入口
`hil/ws63-a5b-response-bound.sh` 使用本机 `0600` 凭据文件构建当前 release closure 的
`wifi_connectivity`，冻结 ELF/profile identity，只烧录一次，再把 20 次 nRST capture
交给同一 parser 的
`--stage connectivity --require-contract --max-runner-step-ms 100`。只要缺少完整
connectivity/A5B trailer、event/backend error 非零、出现 blocking fallback 或单步超过
100 ms，整轮即失败。该 gate 证明 transition/WPA2 profile 的迁移上界，不替代 pure-WPA3。

| 变量 | 默认 | 说明 |
|------|------|------|
| `PORT` | 必填 | WS63 UART0 |
| `PROBE_RS_PROBE` | 空 | `probe-rs --probe` 选择器；多板实验必须与 `PORT` 指向同一 rig |
| `JLINK_SERIAL` | 空 | J-Link 十进制序列号；多板实验必须与 `PROBE_RS_PROBE`、`PORT` 指向同一 rig |
| `WS63_WIFI_PASSPHRASE` | 必填 | 仅由 self-hosted secret 注入，不写入仓库或日志 |
| `WS63_WIFI_ENV_FILE` | 空 | 本地手动 HIL 的 `0600` 普通文件；只接受 `WS63_WIFI_SSID=...` 和 `WS63_WIFI_PASSPHRASE=...`，不执行 shell 内容，默认保留且不能与直接环境变量混用 |
| `WS63_WIFI_ENV_FILE_DISPOSITION` | `keep` | `keep` 保留本机凭据供后续复用；`delete` 在成功读取后删除真正的一次性文件 |
| `WS63_CONNECTIVITY_PROFILE` | `upstream-wpa2` | `upstream-wpa2` / `upstream-wpa3` / `vendor-wpa2`；正式 upstream 验证使用公开 facade 的 plain Cargo lane |
| `WS63_CONNECTIVITY_EXPECT` | `full` | `full` 保持完整 connect/DHCP/local ARP/route disposition/summary/renew gate；有默认路由时还要求 public UDP DNS，没有默认路由时要求精确 SKIP；`init-scan` 使用公开 fixture，仅证明 image/startup/RF init/scan/native runner，不需要 AP secret |
| `WS63_WIFI_AP_MODE` | 空 | `upstream-wpa3` 必须显式为 `transition` 或 `pure-wpa3` |
| `WS63_WPA_ARCHIVE` | 公开 release asset | 可覆盖为 runner 本地缓存路径；内容仍须匹配固定 hash |
| `PROBE_SPEED` | `3000` | 首选 WS63 download 速率；始终保留完整 readback verify。2026-08-03 的 r8 RF init/scan 镜像在 3 MHz 完整验证成功（98.95 秒）；历史上大镜像曾出现 page timeout，失败时依次降到 1000/500 kHz，不得关闭 verify |
| `MONITOR` | `60` | 覆盖 connect、本地 neighbor、可选 UDP DNS 与 smoke-only 20 秒 DHCP lease renew 的 UART 窗口 |
| `EVIDENCE_DIR` | `/private/tmp/ws63-connectivity-smoke-<timestamp>` | 保存 UART、artifact identity 与严格 contract summary；必须为空 |

## 环境变量

### `flash.sh`

烧录方式选 `METHOD=`（默认 `probe-rs`）。

| 变量 | 默认 | 适用 | 说明 |
|------|------|------|------|
| `METHOD` | `probe-rs` | - | `probe-rs`（验证主路径）或 `hisiflash`（厂商路径） |
| `CHIP_KIND` | `ws63` | 共享 | `ws63` / `bs21`，决定默认 app 分区地址 |
| `WS63_RS` | 脚本父目录 | 共享 | ws63-rs 检出根 |
| `CHIP` | `WS63` | probe-rs | probe-rs `--chip` 目标 |
| `PROBE_RS_YAML` | 必填 | probe-rs | fork 的芯片描述 YAML（`HiSilicon_WS63.yaml`） |
| `BASE_ADDRESS` | 未设 | probe-rs | 可选 app 分区覆盖值；未设则读取 `hisi-fwpkg plan` 的 `base_addr` |
| `PROBE_RS` | `probe-rs` | probe-rs | probe-rs 二进制名 |
| `PROBE_RS_PROBE` | 空 | probe-rs | `probe-rs --probe` 选择器；多探针时必填 |
| `PROBE_SPEED` | `2000` | probe-rs | 调试传输时钟，单位 kHz |
| `PORT` | 自动探测 | hisiflash | 串口（导出为 `HISIFLASH_PORT`） |
| `BAUD` | hisiflash 默认 921600 | hisiflash | 烧录波特（`HISIFLASH_BAUD`） |
| `LOADERBOOT` | 必填 | hisiflash | 厂商 LoaderBoot 二进制（取自 fbb_ws63 产物） |
| `ADDRESS` | 必填 | hisiflash | 程序写入 flash 偏移（对照分区表确认；WS63 常见 `0x230000`） |
| `HISIFLASH` | `hisiflash` | hisiflash | hisiflash 二进制名 |

### `hil-smoke.sh`（在 `flash.sh` 变量之外另加）

| 变量 | 默认 | 说明 |
|------|------|------|
| `PORT` | 必填 | 板子 UART0（`/dev/ttyUSBx`） |
| `SETTLE` | `4` | 每次烧录后读 UART 的秒数 |
| `UART_BAUD` | `115200` | 示例 UART0 波特（8N1） |
| `MONITOR` | raw read `$PORT` | 打印原始 UART 到 stdout 的命令（覆盖适配器读法） |
| `HISIFLASH` | `hisiflash` | hisiflash 二进制名 |

### `pack.sh`

| 变量 | 默认 | 说明 |
|------|------|------|
| `CHIP` | `ws63` | 目标芯片（`ws63` / `bs21`），决定 app 分区地址 |
| `APP_ADDR` | 未设 | 覆盖 app 分区 flash 地址（如 `0x230000`） |
| `FWPKG` | 未设 | 非空则同时产出 `.fwpkg`（厂商 hisiflash 路径） |
| `HISI_FWPKG` | `hisi-fwpkg` | hisi-fwpkg 二进制名 |
| `WS63_RS` | 脚本父目录 | ws63-rs 检出根 |

默认 app 分区地址由 `hisi-fwpkg plan` 决定：ws63 `0x00230000`、bs21 `0x00090000`。

### `cargo-run-hw.sh`（cargo runner）

Cargo 以 `<runner> <built-elf>` 调用，脚本执行 `hisi-fwpkg plan --image-output`，再用 probe-rs
`download --binary-format bin --base-address <plan.base_addr>` 写入；设了 `PORT` 时会流式读取 UART0。

| 变量 | 默认 | 说明 |
|------|------|------|
| `PROBE_RS` | `probe-rs` | probe-rs 二进制名 |
| `PROBE_RS_PROBE` | 空 | `probe-rs --probe` 选择器；多探针时必填 |
| `PROBE_CHIP` | `WS63` | probe-rs `--chip` 值 |
| `PROBE_YAML` | 空 | `--chip-description-path` YAML |
| `PROBE_SPEED` | `2000` | 调试传输时钟，单位 kHz |
| `HISI_FWPKG` | `hisi-fwpkg` | hisi-fwpkg 二进制名 |
| `PORT` | 无 | 复位后流式 UART0 的端口 |
| `JLINK_SERIAL` | 空 | 硬件 nRST 使用的 J-Link 十进制序列号；多探针时必填 |
| `UART_BAUD` | `115200` | 流式 UART 波特 |
| `MONITOR` | `10` | 流式 UART 秒数 |

> 启用：`CARGO_TARGET_RISCV32IMFC_UNKNOWN_NONE_ELF_RUNNER=hil/cargo-run-hw.sh cargo run -p blinky --release`
>（或 `just run-hw`）。

多板 HIL 的 rig 身份是
`PROBE_RS_PROBE + JLINK_SERIAL + PORT`。三者必须绑定同一块板，不能把系统 USB
枚举顺序当作稳定身份；reset matrix 同样接受 `JLINK_SERIAL`，因此不会复位一块板却读取
另一块板的 UART。
