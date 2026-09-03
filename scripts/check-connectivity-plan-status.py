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


ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT / "docs" / "plan" / "hisi-connectivity-stack.md"
REGISTRY = ROOT / "docs" / "plan" / "README.md"
ROADMAP = ROOT / "ROADMAP.md"

REQUIRED_STATUS = (
    "U8 stable-graduation review 已完成并给出 no-go",
    "U8R facade-boundary remediation 已闭合",
    "当前唯一 WIP 槽位是产品方向决策 gate",
    "没有自动激活的实现里程碑",
)
REQUIRED_REGISTRY = (
    "| [Connectivity 全栈](hisi-connectivity-stack.md) | 执行中 | P0 |",
    "当前唯一 WIP 槽位是产品方向决策 gate",
    "未显式决策前不启动实现",
)
REQUIRED_ROADMAP = (
    "No new major WIP is activated automatically",
    "coexistence does not graduate without a new product/API review",
)
ALLOWED_OPEN_ITEMS = {
    "**条件触发 -- 第二芯片隔离**",
    "`ws63-rf-rs` facade 继续保留一个 migration release",
}
STALE_ACTIVE_CLAIMS = (
    "当前 U2 WIP",
    "当前 Radio UX/API WIP",
    "当前 A3/A4 gate",
)
OPEN_ITEM = re.compile(r"^- \[ \] (.+?)(?:：|；|$)", re.MULTILINE)
EVIDENCE_LINK = re.compile(r"\]\((evidence/[^)#?]+\.md)(?:#[^)]+)?\)")


def fail(errors: list[str]) -> int:
    for error in errors:
        print(f"connectivity-plan-status: {error}", file=sys.stderr)
    return 1


def status_section(text: str) -> str:
    match = re.search(r"^## 状态\n(?P<body>.*?)(?=^## )", text, re.MULTILINE | re.DOTALL)
    return match.group("body") if match else ""


def main() -> int:
    plan = PLAN.read_text(encoding="utf-8")
    registry = REGISTRY.read_text(encoding="utf-8")
    roadmap = ROADMAP.read_text(encoding="utf-8")
    status = status_section(plan)
    errors: list[str] = []

    if not status:
        errors.append("计划缺少可解析的 `## 状态` 段")
    for statement in REQUIRED_STATUS:
        if statement not in status:
            errors.append(f"状态段缺少当前事实：{statement}")

    registry_row = next(
        (line for line in registry.splitlines() if "(hisi-connectivity-stack.md)" in line),
        "",
    )
    for statement in REQUIRED_REGISTRY:
        if statement not in registry_row:
            errors.append(f"计划注册表条目缺少：{statement}")

    roadmap_head = roadmap.split("## Completed", 1)[0]
    for statement in REQUIRED_ROADMAP:
        if statement not in roadmap_head:
            errors.append(f"ROADMAP 当前窗口缺少：{statement}")

    open_items = set(OPEN_ITEM.findall(plan))
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
        f"({len(evidence_links)} evidence links, {len(open_items)} conditional items)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
