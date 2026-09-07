#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""检查机器可读计划注册表、Markdown 镜像和顶层规划语言。"""

from __future__ import annotations

from pathlib import Path
import re
import sys
import tomllib


ROOT = Path(__file__).resolve().parents[1]
PLAN_DIR = ROOT / "docs" / "plan"
REGISTRY_DATA = PLAN_DIR / "registry.toml"
REGISTRY_MARKDOWN = PLAN_DIR / "README.md"
SCHEMA = "hispark-rs/plan-registry/v1"
STATUS_LABELS = {
    "active": "执行中",
    "decision-pending": "决策待定",
    "supporting": "配套工作",
    "triggered": "条件触发",
    "deferred": "延期",
    "completed": "已完成",
}
PRIORITIES = {"P0", "P1", "P2", "P3", "Done"}
ID = re.compile(r"[a-z0-9][a-z0-9-]*")
ROW = re.compile(
    r"^\| \[[^]]+\]\(([^)]+\.md)\) \| "
    r"(执行中|决策待定|配套工作|条件触发|延期|已完成) \| "
    r"(P0|P1|P2|P3|Done) \|"
)
STATUS_MARKERS = ("## 状态", "## 状态与", "**状态：**")
HAN = re.compile(r"[\u3400-\u9fff]")
ASCII_PROSE = re.compile(r"[A-Za-z]{3,}")
INLINE_CODE = re.compile(r"`[^`]*`")
LINK_TARGET = re.compile(r"\]\([^)]+\)")


def _strings(value: object, field: str, errors: list[str]) -> list[str]:
    if isinstance(value, list) and all(isinstance(item, str) for item in value):
        return value
    errors.append(f"{field} 必须是字符串数组")
    return []


def _safe_repo_path(root: Path, value: str) -> Path | None:
    candidate = (root / value).resolve()
    try:
        candidate.relative_to(root.resolve())
    except ValueError:
        return None
    return candidate


def _cycle(plans: dict[str, dict[str, object]]) -> list[str] | None:
    visiting: list[str] = []
    visited: set[str] = set()

    def visit(plan_id: str) -> list[str] | None:
        if plan_id in visiting:
            start = visiting.index(plan_id)
            return [*visiting[start:], plan_id]
        if plan_id in visited:
            return None
        visiting.append(plan_id)
        for dependency in plans[plan_id].get("depends_on", []):
            if dependency in plans:
                found = visit(dependency)
                if found is not None:
                    return found
        visiting.pop()
        visited.add(plan_id)
        return None

    for plan_id in plans:
        found = visit(plan_id)
        if found is not None:
            return found
    return None


def _validate_markdown_mirror(
    plans: dict[str, dict[str, object]], markdown: Path, errors: list[str]
) -> None:
    rows: dict[str, tuple[str, str]] = {}
    for number, line in enumerate(markdown.read_text(encoding="utf-8").splitlines(), 1):
        match = ROW.match(line)
        if not match:
            continue
        path, status, priority = match.groups()
        if path in rows:
            errors.append(f"README.md:{number} 重复登记 {path}")
            continue
        rows[path] = (status, priority)

    expected_paths = {str(plan["path"]) for plan in plans.values()}
    for path in sorted(expected_paths - set(rows)):
        errors.append(f"README.md 缺少 registry.toml 计划镜像 {path}")
    for path in sorted(set(rows) - expected_paths):
        errors.append(f"README.md 含有 registry.toml 未登记计划 {path}")
    for plan in plans.values():
        path = str(plan["path"])
        expected = (STATUS_LABELS[str(plan["status"])], str(plan["priority"]))
        if rows.get(path) != expected:
            errors.append(
                f"README.md 中 {path} 状态漂移：期望 {expected}，实际 {rows.get(path)}"
            )


def _validate_plan_language(plan_dir: Path, errors: list[str]) -> None:
    stale_claims = {
        "#active-window-now-a4": "应链接当前决策窗口 anchor",
        "#active-window-now-a3-next-a4": "应链接当前决策窗口 anchor",
        "Q3-Q4 仍是 A3 gate": "当前 roadmap 已关闭 A3/Q3/Q4",
    }
    for path in sorted(plan_dir.glob("*.md")):
        if path.name == "README.md":
            continue
        text = path.read_text(encoding="utf-8")
        head = "\n".join(text.splitlines()[:20])
        if not any(marker in head for marker in STATUS_MARKERS):
            errors.append(f"{path.name} 的前 20 行没有中文状态声明")
        for needle, guidance in stale_claims.items():
            if needle in text:
                errors.append(f"{path.name} 包含过期表述 {needle!r}；{guidance}")

    # evidence/ 保存历史证据原文，不参与中文规划正文检查。
    for path in sorted(plan_dir.glob("*.md")):
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
                errors.append(
                    f"{path.name}:{block_line} 解释性正文必须包含中文：{joined}"
                )
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
            if re.match(r"^(?:[-*+]\s+|\d+\.\s+|\|)", stripped):
                check_block()
            if not block:
                block_line = number
            block.append(stripped)
        check_block()


def validate_registry(
    data: dict[str, object],
    *,
    root: Path = ROOT,
    plan_dir: Path = PLAN_DIR,
    markdown: Path | None = REGISTRY_MARKDOWN,
    check_language: bool = True,
) -> list[str]:
    errors: list[str] = []
    if data.get("schema") != SCHEMA:
        errors.append(f"不支持的 schema：{data.get('schema')!r}")
    maximum = data.get("max_active_implementations")
    if not isinstance(maximum, int) or maximum < 0:
        errors.append("max_active_implementations 必须是非负整数")
        maximum = 0

    evidence_by_id: dict[str, str] = {}
    evidence_entries = data.get("evidence")
    if not isinstance(evidence_entries, list):
        errors.append("evidence 必须是数组")
        evidence_entries = []
    for index, entry in enumerate(evidence_entries):
        if not isinstance(entry, dict):
            errors.append(f"evidence[{index}] 必须是对象")
            continue
        evidence_id = entry.get("id")
        path = entry.get("path")
        if not isinstance(evidence_id, str) or not ID.fullmatch(evidence_id):
            errors.append(f"evidence[{index}] ID 非法：{evidence_id!r}")
            continue
        if evidence_id in evidence_by_id:
            errors.append(f"重复 evidence ID：{evidence_id}")
            continue
        if not isinstance(path, str) or not path:
            errors.append(f"evidence {evidence_id} 缺少 path")
            continue
        resolved = _safe_repo_path(root, path)
        if resolved is None or not resolved.is_file():
            errors.append(f"evidence {evidence_id} 路径不存在或越界：{path}")
        evidence_by_id[evidence_id] = path

    plans_by_id: dict[str, dict[str, object]] = {}
    paths: set[str] = set()
    plan_entries = data.get("plan")
    if not isinstance(plan_entries, list):
        errors.append("plan 必须是数组")
        plan_entries = []
    for index, entry in enumerate(plan_entries):
        if not isinstance(entry, dict):
            errors.append(f"plan[{index}] 必须是对象")
            continue
        plan_id = entry.get("id")
        path = entry.get("path")
        status = entry.get("status")
        priority = entry.get("priority")
        if not isinstance(plan_id, str) or not ID.fullmatch(plan_id):
            errors.append(f"plan[{index}] ID 非法：{plan_id!r}")
            continue
        if plan_id in plans_by_id:
            errors.append(f"重复 plan ID：{plan_id}")
            continue
        plans_by_id[plan_id] = entry
        if not isinstance(path, str) or Path(path).name != path or not path.endswith(".md"):
            errors.append(f"plan {plan_id} path 必须是顶层 Markdown 文件名：{path!r}")
        elif path in paths:
            errors.append(f"重复 plan path：{path}")
        else:
            paths.add(path)
        if status not in STATUS_LABELS:
            errors.append(f"plan {plan_id} 状态非法：{status!r}")
        if priority not in PRIORITIES:
            errors.append(f"plan {plan_id} 优先级非法：{priority!r}")
        if not isinstance(entry.get("owner"), str) or not entry["owner"]:
            errors.append(f"plan {plan_id} 缺少 owner")
        if not isinstance(entry.get("trigger"), str) or not entry["trigger"]:
            errors.append(f"plan {plan_id} 缺少 trigger")
        dependencies = _strings(entry.get("depends_on"), f"plan {plan_id}.depends_on", errors)
        evidence = _strings(entry.get("evidence"), f"plan {plan_id}.evidence", errors)
        if plan_id in dependencies:
            errors.append(f"plan {plan_id} 不能依赖自身")
        if status == "completed":
            if priority != "Done":
                errors.append(f"已完成计划 {plan_id} 必须使用 Done 优先级")
            if not evidence:
                errors.append(f"已完成计划 {plan_id} 必须绑定关闭证据")
        elif priority == "Done":
            errors.append(f"未完成计划 {plan_id} 不能使用 Done 优先级")

    markdown_plans = {
        path.name for path in plan_dir.glob("*.md") if path.name != "README.md"
    }
    for path in sorted(markdown_plans - paths):
        errors.append(f"{path} 未登记到 registry.toml")
    for path in sorted(paths - markdown_plans):
        errors.append(f"registry.toml 登记了不存在的计划 {path}")

    for plan_id, entry in plans_by_id.items():
        for dependency in entry.get("depends_on", []):
            if dependency not in plans_by_id:
                errors.append(f"plan {plan_id} 依赖未知 ID：{dependency}")
        for evidence_id in entry.get("evidence", []):
            if evidence_id not in evidence_by_id:
                errors.append(f"plan {plan_id} 引用未知 evidence ID：{evidence_id}")

    found_cycle = _cycle(plans_by_id)
    if found_cycle is not None:
        errors.append(f"计划依赖存在环：{' -> '.join(found_cycle)}")

    active = [
        plan_id
        for plan_id, entry in plans_by_id.items()
        if entry.get("status") == "active"
    ]
    if len(active) > maximum:
        errors.append(f"活动实现超过 WIP 上限 {maximum}：{active}")
    for plan_id in active:
        if plans_by_id[plan_id].get("priority") != "P0":
            errors.append(f"活动实现 {plan_id} 的优先级必须为 P0")

    decision_pending = [
        plan_id
        for plan_id, entry in plans_by_id.items()
        if entry.get("status") == "decision-pending"
    ]
    for plan_id in decision_pending:
        if plans_by_id[plan_id].get("priority") != "P0":
            errors.append(f"决策待定计划 {plan_id} 的优先级必须为 P0")

    if markdown is not None:
        _validate_markdown_mirror(plans_by_id, markdown, errors)
    if check_language:
        _validate_plan_language(plan_dir, errors)
    return errors


def main() -> int:
    data = tomllib.loads(REGISTRY_DATA.read_text(encoding="utf-8"))
    errors = validate_registry(data)
    if errors:
        for error in errors:
            print(f"计划注册表：{error}", file=sys.stderr)
        return 1
    active = [entry["id"] for entry in data["plan"] if entry["status"] == "active"]
    pending = [
        entry["id"] for entry in data["plan"] if entry["status"] == "decision-pending"
    ]
    print(
        f"计划注册表检查通过：{len(data['plan'])} 份计划，"
        f"活动实现={active or 'none'}，决策待定={pending or 'none'}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
