# ws63-flashboot（实验性）

> ⚠️ **实验性 / 学习用途 —— 不是安全启动，不要用于生产。** ⚠️

本 crate 是对 fbb_ws63 `flashboot_ws63/startup/main.c` 的 Rust 重写尝试，用于学习 WS63 的二级
引导流程（SFC 初始化、时钟切换、镜像头解析、跳转 app）。它**有意被排除在默认构建之外**
（见根 `Cargo.toml` 的 `default-members`），且 `publish = false`。

此状态已在 [ROADMAP.md](../../../ROADMAP.md) 阶段 0 确定（2026-05），并决定降优先级（见「冻结 / 降优先级」）：生产启动应复用原厂 flashboot，本 crate 仅作学习用途。

## 为什么不要用于生产

| 能力 | 原厂 flashboot（fbb_ws63） | 本 crate |
|------|--------------------------|----------|
| 镜像**真实性**（签名验签） | ECC-bp256 / SM2，根密钥在 efuse | ❌ **无**，只比对镜像头里的 SHA256（攻击者可重算） |
| 镜像头布局 | 与硅片一致 | ✅ 已对齐 fbb_ws63 `secure_verify_boot.h`（ECC256：`code_area_len`@+0x24、`code_area_hash`@+0x28），但未在真实硬件上对照签名镜像验证 |
| 时钟自适应 `boot_clock_adapt` | 完整 | ⚠️ TODO 空壳 |
| 分区表解析 | `uapi_partition_get_info` | ⚠️ 桩，恒返回 `FLASH_START` |
| 升级/FOTA、镜像解压、flash 在线加密 | 有 | ❌ 无 |

核心问题：**它把原厂的"基于 efuse 根密钥的签名验签"降级成了一个自洽的完整性哈希**——
能写 flash 的人改了镜像、重算 SHA256 写回头部即可以 M 态特权跳进任意代码。这等于没有 secure boot。

## 生产应该怎么做（复用原厂 flashboot）

1. 用 fbb_ws63 的原厂 flashboot 作为二级引导（它已做签名验签 / AB / 升级 / 解压 / flash 加密）。
2. 把本仓库构建出的 Rust 应用镜像，按原厂打包/签名流程烧到原厂 flashboot 加载的 **APP 分区**。
3. Rust 应用使用 `hisi-riscv-rt` 启动 + `hisi-riscv-hal` 驱动，在原厂 flashboot 跳转后接管。

## 显式构建（仅用于实验）

```bash
# 默认构建不含 flashboot；需显式指定（包名是 ws63-flashboot）
cargo build -p ws63-flashboot --release
```

后续若决定继续维护本 crate，整改项见仓库根 [`ROADMAP.md`](../../../ROADMAP.md) 阶段 2，
架构与评审见 [`docs/src/explanation/components/ws63-flashboot.md`](../../../docs/src/explanation/components/ws63-flashboot.md)。
