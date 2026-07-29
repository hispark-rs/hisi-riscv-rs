#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Merge RF profile resources and a hisi-fwpkg FlashPlan into one CI artifact."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import tempfile
from typing import Any


SCHEMA = "hisi-rf-build-report/v1"
RESOURCE_SCHEMAS = {
    "hisi-rf-resource-report/v3",
    "hisi-rf-resource-report/v4",
    "hisi-rf-resource-report/v5",
    "hisi-rf-resource-report/v6",
    "hisi-rf-resource-report/v7",
    "hisi-rf-resource-report/v8",
}
PLAN_KEYS = (
    "base_addr",
    "image_len",
    "body_range",
    "code_area_len",
    "code_area_hash",
    "erase_range",
    "write_chunks",
)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(64 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_object(path: Path, label: str) -> dict[str, Any]:
    with path.open(encoding="utf-8") as source:
        value = json.load(source)
    if not isinstance(value, dict):
        raise ValueError(f"{label} must contain one JSON object")
    return value


def assemble(resource_path: Path, plan_path: Path, elf: Path, image: Path) -> dict[str, Any]:
    resource = load_object(resource_path, "resource report")
    plan = load_object(plan_path, "FlashPlan")
    if resource.get("schema") not in RESOURCE_SCHEMAS:
        raise ValueError(
            f"unsupported resource schema: {resource.get('schema')!r}; "
            f"expected one of {sorted(RESOURCE_SCHEMAS)}"
        )
    if resource["schema"] in {
        "hisi-rf-resource-report/v4",
        "hisi-rf-resource-report/v5",
        "hisi-rf-resource-report/v6",
        "hisi-rf-resource-report/v7",
        "hisi-rf-resource-report/v8",
    }:
        for key in ("runtime_internal_tasks", "task_stack_bytes"):
            value = resource.get(key)
            if not isinstance(value, int) or isinstance(value, bool) or value < 0:
                raise ValueError(
                    f"resource report v4 requires non-negative integer {key}"
                )
    if resource["schema"] in {
        "hisi-rf-resource-report/v5",
        "hisi-rf-resource-report/v6",
        "hisi-rf-resource-report/v7",
        "hisi-rf-resource-report/v8",
    }:
        value = resource.get("shared_rf_arena_bytes")
        if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
            raise ValueError(
                f"resource report {resource['schema'].rsplit('/', 1)[-1]} "
                "requires positive integer shared_rf_arena_bytes"
            )
    if resource["schema"] in {
        "hisi-rf-resource-report/v6",
        "hisi-rf-resource-report/v7",
        "hisi-rf-resource-report/v8",
    }:
        positive_keys = (
            "event_capacity",
            "caller_owned_bytes",
            "control_storage_bytes",
            "radio_state_bytes",
            "crypto_dma_bytes",
            "arena_storage_bytes",
            "main_stack_bytes_required",
            "dynamic_tasks_required",
            "task_stack_bytes",
        )
        non_negative_keys = (
            "composition_handle_bytes",
            "linker_packet_ram_bytes",
        )
        for key in positive_keys:
            value = resource.get(key)
            if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
                raise ValueError(
                    f"resource report v6 requires positive integer {key}"
                )
        for key in non_negative_keys:
            value = resource.get(key)
            if not isinstance(value, int) or isinstance(value, bool) or value < 0:
                raise ValueError(
                    f"resource report v6 requires non-negative integer {key}"
                )
        if resource["arena_storage_bytes"] < resource["shared_rf_arena_bytes"]:
            raise ValueError(
                f"resource report {resource['schema'].rsplit('/', 1)[-1]} "
                "arena_storage_bytes must cover "
                "shared_rf_arena_bytes"
            )
        if not isinstance(resource.get("runtime_resources_calibrated"), bool):
            raise ValueError(
                f"resource report {resource['schema'].rsplit('/', 1)[-1]} "
                "requires boolean runtime_resources_calibrated"
            )
    if resource["schema"] == "hisi-rf-resource-report/v6":
        expected = resource["control_storage_bytes"] + resource["arena_storage_bytes"]
        if resource["caller_owned_bytes"] != expected:
            raise ValueError(
                "resource report v6 caller_owned_bytes must equal "
                "control_storage_bytes + arena_storage_bytes"
            )
    if resource["schema"] == "hisi-rf-resource-report/v7":
        stack_arena = resource.get("task_stack_arena_bytes")
        if not isinstance(stack_arena, int) or isinstance(stack_arena, bool) or stack_arena <= 0:
            raise ValueError(
                "resource report v7 requires positive integer task_stack_arena_bytes"
            )
        if stack_arena < resource["task_stack_bytes"]:
            raise ValueError(
                "resource report v7 task_stack_arena_bytes must cover task_stack_bytes"
            )
        expected = (
            resource["control_storage_bytes"]
            + resource["arena_storage_bytes"]
            + stack_arena
        )
        if resource["caller_owned_bytes"] != expected:
            raise ValueError(
                "resource report v7 caller_owned_bytes must equal control_storage_bytes "
                "+ arena_storage_bytes + task_stack_arena_bytes"
            )
    if resource["schema"] == "hisi-rf-resource-report/v8":
        for key in ("runtime_object_headroom_bytes", "runtime_arena_bytes"):
            value = resource.get(key)
            if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
                raise ValueError(
                    f"resource report v8 requires positive integer {key}"
                )
        if (
            resource["runtime_arena_bytes"]
            < resource["task_stack_bytes"] + resource["runtime_object_headroom_bytes"]
        ):
            raise ValueError(
                "resource report v8 runtime_arena_bytes must cover task_stack_bytes "
                "+ runtime_object_headroom_bytes"
            )
        expected = (
            resource["control_storage_bytes"]
            + resource["arena_storage_bytes"]
            + resource["runtime_arena_bytes"]
        )
        if resource["caller_owned_bytes"] != expected:
            raise ValueError(
                "resource report v8 caller_owned_bytes must equal control_storage_bytes "
                "+ arena_storage_bytes + runtime_arena_bytes"
            )
    missing = [key for key in PLAN_KEYS if key not in plan]
    if missing:
        raise ValueError(f"FlashPlan is missing keys: {missing}")
    if not elf.is_file():
        raise ValueError(f"ELF does not exist: {elf}")
    if not image.is_file():
        raise ValueError(f"image does not exist: {image}")
    if plan["image_len"] != image.stat().st_size:
        raise ValueError("FlashPlan image_len does not match the generated image")

    resolved_resource = dict(resource)
    resolved_resource["flash_bytes"] = plan["image_len"]
    return {
        "schema": SCHEMA,
        "profile": resource.get("profile"),
        "profile_revision": resource.get("profile_revision"),
        "resource": resolved_resource,
        "artifact": {
            "elf_name": elf.name,
            "elf_file_bytes": elf.stat().st_size,
            "elf_sha256": sha256(elf),
            "image_name": image.name,
            "image_bytes": image.stat().st_size,
            "image_sha256": sha256(image),
            "flash_plan_sha256": sha256(plan_path),
            "flash_plan": {key: plan[key] for key in PLAN_KEYS},
        },
    }


def write_report(report: dict[str, Any], output: Path) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(
        json.dumps(report, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="hisi-rf-build-report-") as directory:
        root = Path(directory)
        elf = root / "firmware.elf"
        image = root / "firmware.img"
        resource_path = root / "resource.json"
        plan_path = root / "plan.json"
        output = root / "build-report.json"
        elf.write_bytes(b"ELF fixture")
        image.write_bytes(b"image fixture")
        resource_path.write_text(
            json.dumps(
                {
                    "schema": "hisi-rf-resource-report/v8",
                    "profile": "wifi-wpa2-smoltcp",
                    "profile_revision": "fixture-v1",
                    "runtime_contract": "hisi-rf-rtos-driver/v1.4-ported-cooperative",
                    "task_admission": "owner-bound-slot-stack-reservation",
                    "main_stack_bytes_required": 0x8000,
                    "event_capacity": 8,
                    "caller_owned_bytes": 311_776,
                    "control_storage_bytes": 8_608,
                    "composition_handle_bytes": 0,
                    "radio_state_bytes": 2_216,
                    "crypto_dma_bytes": 6_336,
                    "arena_storage_bytes": 114_240,
                    "linker_packet_ram_bytes": 0x24000,
                    "dynamic_tasks_required": 7,
                    "runtime_internal_tasks": 2,
                    "task_stack_bytes": 7 * 24 * 1024,
                    "runtime_object_headroom_bytes": 16 * 1024,
                    "runtime_arena_bytes": 188_928,
                    "shared_rf_arena_bytes": 114_176,
                    "flash_bytes": None,
                    "runtime_resources_calibrated": False,
                }
            ),
            encoding="utf-8",
        )
        plan = {
            "base_addr": 0x230000,
            "image_len": image.stat().st_size,
            "body_range": {"start": 0x230300, "end": 0x230400},
            "code_area_len": 0x100,
            "code_area_hash": "00" * 32,
            "erase_range": {"start": 0x230000, "end": 0x231000},
            "write_chunks": [{"address": 0x230000, "length": image.stat().st_size}],
        }
        plan_path.write_text(json.dumps(plan), encoding="utf-8")

        report = assemble(resource_path, plan_path, elf, image)
        write_report(report, output)
        persisted = load_object(output, "build report")
        assert persisted["schema"] == SCHEMA
        assert persisted["resource"]["flash_bytes"] == image.stat().st_size
        assert persisted["resource"]["main_stack_bytes_required"] == 0x8000
        assert (
            persisted["resource"]["task_admission"]
            == "owner-bound-slot-stack-reservation"
        )
        assert persisted["resource"]["task_stack_bytes"] == 7 * 24 * 1024
        assert persisted["resource"]["runtime_object_headroom_bytes"] == 16 * 1024
        assert persisted["resource"]["runtime_arena_bytes"] == 188_928
        assert persisted["resource"]["shared_rf_arena_bytes"] == 114_176
        assert persisted["resource"]["caller_owned_bytes"] == 311_776
        assert persisted["artifact"]["elf_name"] == elf.name
        assert persisted["artifact"]["image_name"] == image.name
        assert str(root) not in output.read_text(encoding="utf-8")

        resource_v8 = load_object(resource_path, "resource report")
        resource_v7 = dict(resource_v8)
        resource_v7["schema"] = "hisi-rf-resource-report/v7"
        resource_v7["task_stack_arena_bytes"] = 172_544
        resource_v7["caller_owned_bytes"] = (
            resource_v7["control_storage_bytes"]
            + resource_v7["arena_storage_bytes"]
            + resource_v7["task_stack_arena_bytes"]
        )
        resource_v7.pop("runtime_object_headroom_bytes")
        resource_v7.pop("runtime_arena_bytes")
        resource_path.write_text(json.dumps(resource_v7), encoding="utf-8")
        assert assemble(resource_path, plan_path, elf, image)["resource"]["schema"].endswith(
            "/v7"
        )

        resource_v6 = dict(resource_v7)
        resource_v6["schema"] = "hisi-rf-resource-report/v6"
        resource_v6["caller_owned_bytes"] = (
            resource_v6["control_storage_bytes"] + resource_v6["arena_storage_bytes"]
        )
        resource_v6.pop("task_stack_arena_bytes")
        resource_path.write_text(json.dumps(resource_v6), encoding="utf-8")
        assert assemble(resource_path, plan_path, elf, image)["resource"]["schema"].endswith(
            "/v6"
        )

        resource_path.write_text(json.dumps(resource_v8), encoding="utf-8")
        plan["image_len"] += 1
        plan_path.write_text(json.dumps(plan), encoding="utf-8")
        try:
            assemble(resource_path, plan_path, elf, image)
        except ValueError as error:
            assert "image_len" in str(error)
        else:
            raise AssertionError("mismatched FlashPlan image_len was accepted")

        resource_v8["schema"] = "hisi-rf-resource-report/v9"
        resource_path.write_text(json.dumps(resource_v8), encoding="utf-8")
        try:
            assemble(resource_path, plan_path, elf, image)
        except ValueError as error:
            assert "unsupported resource schema" in str(error)
        else:
            raise AssertionError("unknown resource report schema was accepted")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--resource", type=Path)
    parser.add_argument("--plan", type=Path)
    parser.add_argument("--elf", type=Path)
    parser.add_argument("--image", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if not args.self_test and not all(
        (args.resource, args.plan, args.elf, args.image, args.output)
    ):
        parser.error("--resource, --plan, --elf, --image, and --output are required")
    return args


def main() -> int:
    args = parse_args()
    if args.self_test:
        self_test()
        print("radio build report self-test OK")
        return 0
    report = assemble(args.resource, args.plan, args.elf, args.image)
    write_report(report, args.output)
    print(
        f"radio build report OK: {args.output} "
        f"({report['artifact']['image_bytes']} flash bytes)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
