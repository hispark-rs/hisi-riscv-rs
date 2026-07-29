# RF 错误与诊断契约

`hisi-rf` 的公共错误可通过 `Error::diagnostic()` 或
`InitError::diagnostic()` 转为 allocation-free 的 `Diagnostic`。当前机器可读 schema 是
`hisi-rf-error/v3`；其字段只包含稳定分类、阶段、恢复动作、数值 backend code、profile
revision 和最多 4 项数值 trace，不包含 SSID、passphrase、密钥或任意 backend 文本。

## 三层时间边界

| 层次 | 类型/事实源 | 超时结果 | 语义 |
|---|---|---|---|
| 协议操作 | `OperationTimeout` | `operation.timeout` | scan/connect 等协议状态机在约定时间内没有完成；取消必须进入 backend cleanup |
| backend 生命周期 | `BackendTimeout` | `backend.timeout` | 初始化、断开或 RTOS/vendor 有界等待没有完成 |
| 应用等待 | 应用自己的 deadline 类型 | 应用自定义结果 | 调用方不愿继续等待某个 Future；它不是 backend 错误，也不能覆盖内层诊断 |

丢弃一个已被 backend 接受的 operation Future 会请求取消；实际 vendor/transport cleanup
由 runner 在普通任务上下文推进，`Drop` 本身不执行 vendor 调用。模板使用独立的
`ApplicationWaitDeadline`，并以 `hisi-rf-application-wait/v1` 输出脱敏 marker；该 schema
属于生成应用，不属于 `hisi-rf-error/v3`。

## 字段

| 字段 | 含义 |
|---|---|
| `code` | 稳定错误标识；应用和工具优先匹配此字段 |
| `stage` | 失败发生在 initialize、scan、associate、SAE、EAPOL、PMF、runtime 等哪个阶段 |
| `action` | 可执行的下一步；不是“重试一切”的模糊建议 |
| `backend_code` | 芯片或 backend 的无损数值状态；未知值也必须保留 |
| `profile_revision` | 生成错误的 chip/profile revision；没有事实时为 `null` |
| `trace` | 固定容量的数值上下文；`trace_truncated` 表示还有条目未保留 |
| `docs` | 本页中的稳定 fragment；工具应与当前手册版本的本页 URL 组合 |

不同来源的数值不能混用：vendor status、IEEE 802.11 status、hostap status、disconnect
reason 和 runtime code 使用不同 `trace.kind`。日志文本只用于现场阅读，不是错误分类的事实源。

<a id="errors-radio-already-initialized"></a>
## `radio.already_initialized`

同一份 radio storage 或硬件资源已被 controller 持有。继续使用已有 controller；不要通过
`steal`、重复初始化或重新构造 token 绕过唯一所有权。

<a id="errors-backend-initialize"></a>
## `backend.initialize`

backend 在进入正常操作前失败。核对 profile revision、`backend_code` 和 trace，再重新创建并
初始化 controller；重复失败应保留完整诊断，而不是只报告“初始化失败”。

<a id="errors-backend-busy"></a>
## `backend.busy`

已有控制面操作占用 backend。等待当前 bounded operation 完成后重试；不要并发发起第二个
scan/connect/disconnect。

<a id="errors-backend-timeout"></a>
## `backend.timeout`

backend 初始化、断开或 RTOS/vendor 有界等待超过 deadline。先看 `stage` 和 trace，确认
事件循环、timer、IRQ 和硬件生命周期；不要把它与协议 operation timeout 或应用等待 deadline
混为同一个保证。

<a id="errors-operation-timeout"></a>
## `operation.timeout`

scan/connect 等协议操作超过自身的 `OperationTimeout`。保留 backend code 与 trace，按
`associate`、`eapol`、`pmf` 等 stage 定位；不要只增大应用总等待时间掩盖状态机缺口。终止
Future 后，runner 仍必须完成有界取消并回收 owner、queue slot、timer 和 key state。

<a id="errors-operation-cancelled"></a>
## `operation.cancelled`

调用方取消了尚未完成的操作。等待取消完成或重新发起操作；旧 operation generation 的完成
不得被解释为新操作成功。

<a id="errors-resource-unavailable"></a>
## `resource.unavailable`

选定 profile 的 bounded resource 不足。查看 `resource_required` / `resource_available` trace，
增加 caller-owned storage/arena 或选择更小且已验证的 profile。运行期 heap watermark 只是校准
观测，不等价于初始化前 reservation。

<a id="errors-wifi-unsupported-security"></a>
## `wifi.unsupported_security`

扫描结果、station config 与编译选择的 Personal security profile 不兼容。选择支持该网络的
WPA2/WPA3 profile；不要把 unknown/protected BSS 静默降级成开放网络。

<a id="errors-wifi-connection-failed"></a>
## `wifi.connection_failed`

认证、关联或授权失败。根据 `stage` 和来源明确的 trace 判断是 authentication、association、
SAE、EAPOL 还是 PMF；IEEE status 30 表示 temporary rejection/PMF 恢复路径，不等于 WPA 四次
握手失败。

<a id="errors-backend-other"></a>
## `backend.other`

当前稳定 schema 尚无更精确分类。必须保留 `backend_code`、profile revision 和 trace，并按数值
来源定位；未知 code 不得被丢弃或猜测映射。

<a id="errors-radio-protocol"></a>
## `radio.protocol`

runner 观察到无效的 command/completion 序列。重新创建 controller；若可复现，报告 profile
revision、bounded trace 和操作顺序，不要继续复用可能已失配的控制面状态。
