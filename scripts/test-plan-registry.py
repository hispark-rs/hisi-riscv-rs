#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""计划注册表的正向与 mutation 负测试。"""

from __future__ import annotations

import copy
import importlib.util
from pathlib import Path
import tomllib


ROOT = Path(__file__).resolve().parents[1]
CHECKER_PATH = ROOT / "scripts" / "check-plan-registry.py"
SPEC = importlib.util.spec_from_file_location("check_plan_registry", CHECKER_PATH)
assert SPEC is not None and SPEC.loader is not None
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)
CONNECTIVITY_PATH = ROOT / "scripts" / "check-connectivity-plan-status.py"
CONNECTIVITY_SPEC = importlib.util.spec_from_file_location(
    "check_connectivity_plan_status", CONNECTIVITY_PATH
)
assert CONNECTIVITY_SPEC is not None and CONNECTIVITY_SPEC.loader is not None
CONNECTIVITY = importlib.util.module_from_spec(CONNECTIVITY_SPEC)
CONNECTIVITY_SPEC.loader.exec_module(CONNECTIVITY)
SOURCE = tomllib.loads((ROOT / "docs/plan/registry.toml").read_text())


def errors(data: dict[str, object]) -> list[str]:
    return CHECKER.validate_registry(data, markdown=None, check_language=False)


def require_failure(data: dict[str, object], needle: str) -> None:
    failures = errors(data)
    if not any(needle in failure for failure in failures):
        raise AssertionError(f"expected {needle!r}, got {failures}")


def main() -> None:
    assert errors(copy.deepcopy(SOURCE)) == []

    duplicate = copy.deepcopy(SOURCE)
    duplicate["plan"][1]["id"] = duplicate["plan"][0]["id"]
    require_failure(duplicate, "重复 plan ID")

    cycle = copy.deepcopy(SOURCE)
    cycle["plan"][0]["depends_on"] = [cycle["plan"][1]["id"]]
    require_failure(cycle, "计划依赖存在环")

    no_evidence = copy.deepcopy(SOURCE)
    no_evidence["plan"][-1]["evidence"] = []
    require_failure(no_evidence, "必须绑定关闭证据")

    too_many_active = copy.deepcopy(SOURCE)
    for entry in too_many_active["plan"][:2]:
        entry["status"] = "active"
        entry["priority"] = "P0"
    require_failure(too_many_active, "活动实现超过 WIP 上限")

    unknown_evidence = copy.deepcopy(SOURCE)
    unknown_evidence["plan"][0]["evidence"] = ["missing-evidence"]
    require_failure(unknown_evidence, "引用未知 evidence ID")

    invalid_evidence = copy.deepcopy(SOURCE)
    invalid_evidence["evidence"][0]["path"] = "../../outside.md"
    require_failure(invalid_evidence, "路径不存在或越界")

    hidden_checkbox = "  * [ ] 未登记实现：合法缩进和星号不能绕过检查"
    assert CONNECTIVITY.OPEN_ITEM.findall(hidden_checkbox) == ["未登记实现"]

    active_status = "当前唯一活动里程碑是 NET0"
    assert CONNECTIVITY.check_active_window(SOURCE, active_status) == []
    assert CONNECTIVITY.check_active_window(SOURCE, "决策待定")
    premature = copy.deepcopy(SOURCE)
    premature["plan"][0]["active_milestone"] = "NET1"
    assert CONNECTIVITY.check_active_window(premature, active_status)
    concurrent = copy.deepcopy(SOURCE)
    concurrent["plan"][1]["status"] = "active"
    assert CONNECTIVITY.check_active_window(concurrent, active_status)

    plan = (ROOT / "docs/plan/hisi-connectivity-stack.md").read_text()
    assert CONNECTIVITY.check_net_milestones(plan) == []
    assert CONNECTIVITY.check_net_milestones(plan.replace("| NET1 Embassy Net | queued |", "| NET1 Embassy Net | active |"))
    assert CONNECTIVITY.check_net_milestones(plan.replace("| NET0 L2 | active |", "| NET0 L2 | complete |"))

    print("计划注册表契约测试通过：3 个正向场景，12 个 mutation/解析负场景")


if __name__ == "__main__":
    main()
