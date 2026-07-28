# WS63 RF A5UX 公共 API 基线证据

日期：2026-07-28

## 结论

A5UX 在改变用户 API 形态前，已经建立三层、受版本控制的
`cargo-public-api 0.52.0` 基线：

| Release unit | Feature contract | 基线规模 | Commit |
|---|---|---:|---|
| `hisi-rf-core` | all features | 532 项 | `76cd410` |
| `hisi-rf-ws63` | WPA2 incremental | 498 项 | `25c0bad` |
| `hisi-rf-ws63` | WPA3 incremental | 498 项 | `25c0bad` |
| `hisi-rf` | WS63 named profile + incremental Embassy wait | 107 项可达名字 | `52ba8a2` |

WS63 两个安全 profile 的完整公共面只有 `SelectedProfile` 目标类型这一项预期差异；
facade 的可达名字完全一致。任何其他新增、删除或签名变化都会在 CI 中输出 unified
diff 并失败。

## 事实源

- `hisi-rf-core/.github/public-api/all-features.txt`
- `hisi-rf-ws63/.github/public-api/wpa2-incremental.txt`
- `hisi-rf-ws63/.github/public-api/wpa3-incremental.txt`
- `hisi-rf/.github/public-api/ws63-incremental.txt`

门禁使用固定的 `cargo-public-api 0.52.0`，并先验证独立仓 `Cargo.lock`。基线只能在
有意的 API 变更中更新；普通依赖升级或重构不能顺带刷新快照。

## 验证

- `hisi-rf-core`：
  - 48 项 host test；
  - all-features clippy；
  - standalone package；
  - CI run `30374580131`。
- `hisi-rf-ws63`：
  - WPA2 91 项、WPA3 96 项 host test；
  - WPA2 clippy；
  - 两个 profile 的 snapshot + hidden-type gate；
  - CI run `30374575863`。
- `hisi-rf`：
  - 两个 named profile 的 facade snapshot parity；
  - host test、clippy、standalone package；
  - CI run `30374580182`。

## 边界

facade 自身的 rustdoc JSON 只能看到重新导出的名字，不能展开依赖 crate 中类型的全部
方法和字段。因此不能只保留 facade 快照；`hisi-rf-core` 和 `hisi-rf-ws63` 的完整快照
是同一门槛的必要组成。

这份证据只冻结 API 迁移前的起点，不证明 A5UX 已完成，也不替代最终
`wifi_connectivity` HIL。长期 `init`、typed resources/storage、opaque event capacity、
timeout/cancellation、device-owned MAC 和统一诊断快照仍按 A5UX 清单在最终镜像验收后
逐项迁移。
