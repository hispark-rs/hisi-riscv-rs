# 从 `ws63-rf-rs` 迁移到 `hisi-rf`

面向应用的 WS63 Wi-Fi 入口已经收敛到 `hisi-rf`。旧
`ws63-rf-rs` 仍保留给父仓中的连接性迁移 oracle 和底层诊断示例；新应用不应直接依赖
它，也不应直接依赖 `ws63-radio-sys` 或 `hisi-rf-rtos-driver`。

本页只说明应用迁移。底层 crate 的 archive、ROM patch、OSAL 和 ABI 关系见
[WS63 RF 组件说明](../explanation/components/09-ws63-rf.md)。

## 1. 替换依赖

删除应用 `Cargo.toml` 中的直接底层依赖：

```toml
ws63-rf-rs = { path = "..." }
ws63-radio-sys = { path = "..." }
hisi-rf-rtos-driver = "..."
```

改为一个公开 RF facade，并显式选择芯片和完整 profile：

```toml
[dependencies]
hisi-rf = {
    version = "=0.1.0-alpha.41",
    features = ["chip-ws63", "profile-wifi-wpa2-smoltcp"],
}
hisi-rtos = "0.1.0-alpha.13"
smoltcp = {
    version = "0.13",
    default-features = false,
    features = ["medium-ethernet", "proto-ipv4", "socket-dhcpv4"],
}
```

版本号应以当前 release train 或
[模板生命周期矩阵](https://github.com/hispark-rs/hisi-rs-template/blob/main/docs/lifecycle.md)
为准，不要把本页的示例版本当作跨 release 的范围依赖。

| 旧 feature / 依赖 | 新选择 | 说明 |
| --- | --- | --- |
| `ws63-rf-rs/net` + WPA2 相关 features | `chip-ws63`, `profile-wifi-wpa2-smoltcp` | 当前用户 happy path |
| `ws63-rf-rs` 的 WPA3 / upstream supplicant 组合 | `chip-ws63`, `profile-wifi-wpa3-smoltcp` | 仍属 alpha；transition-mode 已验证，pure-WPA3 gate 尚未闭合 |
| 直接 `hisi-rf-rtos-driver` | `hisi-rtos` | 应用启动 RTOS，但不调用 RF driver service locator |
| 直接 `ws63-radio-sys` | 无 | 由 `hisi-rf` 的 WS63 backend 传递依赖并完成链接 |
| `rf-init-diag` / `rf-eloop-diag` 等底层诊断 | `hisi-rf::Diagnostic` | 低层诊断只保留给 maintainer oracle，不迁移为用户 API |

不要同时选择 WPA2 和 WPA3 profile。缺少 `chip-ws63`、缺少 profile 所需网络后端或
选择冲突时，facade 会在编译期报错。

## 2. 把全局状态改成调用方持有的 storage

旧应用通常由 `ws63-rf-rs` 内部全局状态隐式持有队列、密码硬件 scratch 和 RF heap。
新入口要求应用明确提供有界 storage 和 radio arena：

```rust
const EVENT_CAPACITY: usize = 8;

static RADIO_STORAGE:
    hisi_rf::ws63::Storage<hisi_rf::ws63::SelectedProfile, EVENT_CAPACITY> =
    hisi_rf::ws63::Storage::new();

hisi_rf::ws63::declare_radio_arena!(static RADIO_ARENA);
```

初始化 RTOS 前先 claim 并安装 arena。重复 claim、容量不足或 profile 不匹配会在启动
blob 前返回结构化错误：

```rust
let radio_arena = RADIO_ARENA
    .claim_for::<hisi_rf::ws63::SelectedProfile>()?
    .install()?;
```

完整资源数字由生成项目中的 `just rf-resource-report` 输出；不要从旧示例复制 heap、
task 或 queue 常量。

## 3. 使用 WS63 composition root

旧路径会在应用中构造 `Ws63WifiBackend`，或者分别调用 `config()`、`resources()` 和
底层初始化函数。新路径只组装 HAL token，并通过一个入口创建 controller：

```rust
let resources =
    hisi_rf::ws63::Resources::new(efuse, km, spacc, pke, trng, radio_arena);

let controller =
    hisi_rf::ws63::init(hisi_rf::RadioConfig::default(), resources, &RADIO_STORAGE)?;

let mut wifi = controller.start_runner()?;
```

应用只通过 chip-neutral 控制面继续工作：

- `wifi.controller.initialize().await`
- `wifi.controller.scan(...)`
- `wifi.controller.connect(...)`
- `wifi.device` 接入 `smoltcp`

不要从 `hisi-rf-ws63`、`ws63-radio-sys` 或 blob crate 导入实现类型。它们是 facade 的
内部依赖，不是应用兼容面。

## 4. 启动 runtime，不安装 RF service locator

WS63 用户路径使用 `hisi_rtos::start_with_port(...)` 启动 timer/SWI port。应用可以在
自己的执行模型里轮询 radio future；当前同步模板在 future 返回 `Pending` 时调用
`hisi_rtos::request_reschedule()`，让 radio runner 和 vendor worker 获得调度点。

旧的 `hisi-rf-rtos-driver` 安装、任务创建和 OSAL 映射由 composition root 及其 backend
负责。应用不应再直接注册 driver runtime。

## 5. 验证迁移

最可靠的迁移基线是重新生成一个 Wi-Fi 工程，与现有应用做差分：

```bash
cargo generate --git https://github.com/hispark-rs/hisi-rs-template \
    --name rf-migration-reference \
    --define chip=ws63 \
    --define starter=wifi \
    --silent

cd rf-migration-reference
just build
just rf-resource-report
just image
```

然后检查原应用：

```bash
cargo tree -i ws63-rf-rs
cargo tree -i ws63-radio-sys
cargo tree -i hisi-rf-rtos-driver
```

预期：

- `ws63-rf-rs` 不再出现在用户应用依赖图中；
- `ws63-radio-sys` 和 `hisi-rf-rtos-driver` 即使出现，也只能是
  `hisi-rf -> hisi-rf-ws63` 下的传递实现依赖；
- 应用 manifest 中唯一直接 RF 依赖是 `hisi-rf`；
- `cargo build --release` 不调用 Python、Shell、原厂 GCC 或原厂 SDK；
- `just image` 只在 ELF 完成链接后由 `hisi-fwpkg` 生成可烧录镜像。

真机迁移验收至少保留 `WIFI_INIT_OK`、`WIFI_SCAN_OK`、`WIFI_CONNECT_OK` 和
`WIFI_DHCP_OK`。旧 `wifi_init_smoke` 仍是 maintainer 诊断 oracle，不应作为新应用结构
的复制模板。

## 迁移窗口

`ws63-rf-rs::radio` 已弃用，父仓 0.7.x 保留迁移兼容；删除不会早于父仓 0.8.0。
删除还要求 pure-WPA3 parity gate 和旧 oracle 退役条件闭合。因此，“新应用已经迁移”
不等于“旧 crate 现在可以删除”。
