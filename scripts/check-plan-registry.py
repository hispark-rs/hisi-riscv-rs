#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""检查工程计划注册表、当前计划不变量和顶层规划语言。"""

from __future__ import annotations

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
PLAN_DIR = ROOT / "docs" / "plan"
REGISTRY = PLAN_DIR / "README.md"
ROW = re.compile(
    r"^\| \[[^]]+\]\(([^)]+\.md)\) \| "
    r"(执行中|配套工作|条件触发|延期|已完成) \| "
    r"(P0|P1|P2|P3|Done) \|"
)
STATUS_MARKERS = ("## 状态", "## 状态与", "**状态：**")
HAN = re.compile(r"[\u3400-\u9fff]")
ASCII_PROSE = re.compile(r"[A-Za-z]{3,}")
INLINE_CODE = re.compile(r"`[^`]*`")
LINK_TARGET = re.compile(r"\]\([^)]+\)")


def fail(messages: list[str]) -> None:
    for message in messages:
        print(f"计划注册表：{message}", file=sys.stderr)
    raise SystemExit(1)


errors: list[str] = []
rows: dict[str, tuple[str, str]] = {}

for number, line in enumerate(REGISTRY.read_text(encoding="utf-8").splitlines(), 1):
    match = ROW.match(line)
    if not match:
        continue
    path, status, priority = match.groups()
    if "/" in path or path == "README.md":
        errors.append(f"README.md:{number} 必须链接一个顶层计划文件名")
        continue
    if path in rows:
        errors.append(f"README.md:{number} 重复登记 {path}")
        continue
    rows[path] = (status, priority)

plans = {
    path.name
    for path in PLAN_DIR.glob("*.md")
    if path.name != REGISTRY.name
}
listed = set(rows)
for path in sorted(plans - listed):
    errors.append(f"{path} 未登记到 docs/plan/README.md")
for path in sorted(listed - plans):
    errors.append(f"README.md 登记了不存在的计划 {path}")

active = [path for path, (status, _) in rows.items() if status == "执行中"]
if len(active) != 1:
    errors.append(f"必须恰好有一个执行中计划，实际为 {len(active)} 个：{active}")
elif rows[active[0]][1] != "P0":
    errors.append(f"执行中计划 {active[0]} 的优先级必须为 P0")

for path, (status, priority) in rows.items():
    if status == "已完成" and priority != "Done":
        errors.append(f"已完成计划 {path} 必须使用 Done 优先级")
    if status != "已完成" and priority == "Done":
        errors.append(f"未完成计划 {path} 不能使用 Done 优先级")

for name in sorted(plans):
    head = "\n".join((PLAN_DIR / name).read_text(encoding="utf-8").splitlines()[:20])
    if not any(marker in head for marker in STATUS_MARKERS):
        errors.append(f"{name} 的前 20 行没有中文状态声明")

stale_claims = {
    "#active-window-now-a4": "应链接当前 A5U 执行窗口 anchor",
    "#active-window-now-a3-next-a4": "应链接当前 A5U 执行窗口 anchor",
    "Q3-Q4 仍是 A3 gate": "当前 roadmap 已关闭 A3/Q3/Q4",
}
for path in sorted(PLAN_DIR.glob("*.md")):
    if path == REGISTRY:
        continue
    text = path.read_text(encoding="utf-8")
    for needle, guidance in stale_claims.items():
        if needle in text:
            errors.append(f"{path.name} 包含过期表述 {needle!r}；{guidance}")

# 顶层规划正文统一使用中文。evidence/ 保存历史证据原文，不参与本检查。
# 代码块、inline code、命令、路径、URL 和技术标识符可以保留英文。正文按 Markdown
# 段落/列表项检查，避免把中文列表项中因换行独立出现的技术术语误判成英文段落。
for path in sorted(PLAN_DIR.glob("*.md")):
    in_code = False
    block: list[str] = []
    block_line = 0

    def check_block() -> None:
        if not block:
            return
        joined = " ".join(block)
        visible = INLINE_CODE.sub("", joined)
        visible = LINK_TARGET.sub("]", visible)
        if ASCII_PROSE.search(visible) and not HAN.search(visible):
            errors.append(f"{path.name}:{block_line} 解释性正文必须包含中文：{joined}")
        block.clear()

    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        stripped = line.strip()
        if stripped.startswith("```"):
            check_block()
            in_code = not in_code
            continue
        if in_code:
            continue
        if not stripped or stripped.startswith("<a id="):
            check_block()
            continue
        if stripped.startswith("#"):
            check_block()
            if not HAN.search(stripped):
                errors.append(f"{path.name}:{number} 标题必须包含中文：{stripped}")
            continue

        starts_item = bool(re.match(r"^(?:[-*+]\s+|\d+\.\s+|\|)", stripped))
        if starts_item:
            check_block()
        if not block:
            block_line = number
        block.append(stripped)
    check_block()

if errors:
    fail(errors)

print(f"计划注册表检查通过：{len(plans)} 份计划，执行中={active[0]}")
