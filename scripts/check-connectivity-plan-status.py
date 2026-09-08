#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Enforce the current connectivity plan window and its conditional backlog."""

from __future__ import annotations

from pathlib import Path
import re
import sys
import tomllib


ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT / "docs" / "plan" / "hisi-connectivity-stack.md"
REGISTRY = ROOT / "docs" / "plan" / "registry.toml"

ALLOWED_OPEN_ITEMS = {
    "**条件触发 -- 第二芯片隔离**",
    "`ws63-rf-rs` facade 继续保留一个 migration release",
}
STALE_ACTIVE_CLAIMS = (
    "当前 U2 WIP",
    "当前 Radio UX/API WIP",
    "当前 A3/A4 gate",
)
OPEN_ITEM = re.compile(r"^\s*[-*+]\s+\[ \]\s+(.+?)(?:：|；|$)", re.MULTILINE)
EVIDENCE_LINK = re.compile(r"\]\((evidence/[^)#?]+\.md)(?:#[^)]+)?\)")


def fail(errors: list[str]) -> int:
    for error in errors:
        print(f"connectivity-plan-status: {error}", file=sys.stderr)
    return 1


def status_section(text: str) -> str:
    match = re.search(r"^## 状态\n(?P<body>.*?)(?=^## )", text, re.MULTILINE | re.DOTALL)
    return match.group("body") if match else ""


def check_active_window(registry: dict, status: str) -> list[str]:
    """Keep the approved implementation and its document entry in agreement."""
    errors = []
    entries = registry.get("plan", [])
    connectivity = next(
        (entry for entry in entries if entry.get("id") == "connectivity-stack"), None
    )
    if connectivity is None:
        return ["registry.toml 缺少 connectivity-stack"]
    if connectivity.get("status") != "active":
        errors.append("已批准的 connectivity-stack 必须是 active")
    if connectivity.get("active_milestone") != "NET0":
        errors.append("当前仅批准激活 NET0；后续阶段须先验收并更新契约")
    if "当前唯一活动里程碑是 NET0" not in status:
        errors.append("状态段必须与 registry 的 NET0 活动窗口一致")
    active = [entry.get("id") for entry in entries if entry.get("status") == "active"]
    if active != ["connectivity-stack"]:
        errors.append(f"活动实现必须唯一：{active}")
    return errors


def main() -> int:
    plan = PLAN.read_text(encoding="utf-8")
    registry = tomllib.loads(REGISTRY.read_text(encoding="utf-8"))
    status = status_section(plan)
    errors: list[str] = []

    if not status:
        errors.append("计划缺少可解析的 `## 状态` 段")

    errors.extend(check_active_window(registry, status))

    open_item_list = OPEN_ITEM.findall(plan)
    open_items = set(open_item_list)
    duplicates = sorted(item for item in open_items if open_item_list.count(item) > 1)
    for item in duplicates:
        errors.append(f"重复的条件 backlog checkbox：{item}")
    unexpected = sorted(open_items - ALLOWED_OPEN_ITEMS)
    missing = sorted(ALLOWED_OPEN_ITEMS - open_items)
    for item in unexpected:
        errors.append(f"出现未登记的活动 checkbox：{item}")
    for item in missing:
        errors.append(f"条件 backlog checkbox 丢失或被误标完成：{item}")

    second_chip = re.search(
        r"^- \[ \] \*\*条件触发 -- 第二芯片隔离\*\*.*?(?=^- \[[ x]\]|^#### |^### |^## )",
        plan,
        re.MULTILINE | re.DOTALL,
    )
    if second_chip is None or "当前只有 WS63 backend" not in second_chip.group(0):
        errors.append("第二芯片隔离项必须明确当前只有 WS63 backend")

    facade = re.search(
        r"^- \[ \] `ws63-rf-rs` facade.*?(?=^- \[[ x]\]|^#### |^### |^## )",
        plan,
        re.MULTILINE | re.DOTALL,
    )
    if facade is None or "不早于父仓 v0.8.0" not in facade.group(0):
        errors.append("旧 facade 退役项必须保留父仓 v0.8.0 版本门槛")

    for claim in STALE_ACTIVE_CLAIMS:
        if claim in plan:
            errors.append(f"计划重新出现过期活动状态：{claim}")

    evidence_links = sorted(set(EVIDENCE_LINK.findall(plan)))
    for target in evidence_links:
        path = PLAN.parent / target
        if not path.is_file():
            errors.append(f"证据链接不存在：{target}")

    if errors:
        return fail(errors)

    print(
        "connectivity-plan-status: OK "
        f"({len(evidence_links)} evidence links, {len(open_items)} conditional items, "
        "active implementations=1, milestone=NET0)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
