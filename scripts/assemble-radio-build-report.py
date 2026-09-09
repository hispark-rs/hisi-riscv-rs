#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Merge RF profile resources and a hisi-fwpkg FlashPlan into one CI artifact."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import struct
import tempfile
from typing import Any

from elftools.elf.elffile import ELFFile


SCHEMA = "hisi-rf-build-report/v1"
RESOURCE_SCHEMAS = {
    "hisi-rf-resource-report/v3",
    "hisi-rf-resource-report/v4",
    "hisi-rf-resource-report/v5",
    "hisi-rf-resource-report/v6",
    "hisi-rf-resource-report/v7",
    "hisi-rf-resource-report/v8",
    "hisi-rf-resource-report/v9",
    "hisi-rf-resource-report/v10",
    "hisi-rf-resource-report/v11",
    "hisi-rf-resource-report/v12",
    "hisi-rf-resource-report/v13",
    "hisi-rf-resource-report/v14",
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


def validate_task_resource_tree(resource: dict[str, Any]) -> None:
    if resource["schema"] not in {
        "hisi-rf-resource-report/v10",
        "hisi-rf-resource-report/v11",
        "hisi-rf-resource-report/v12",
        "hisi-rf-resource-report/v13",
        "hisi-rf-resource-report/v14",
    }:
        return

    schema_revision = resource["schema"].rsplit("/", 1)[-1]
    positive_keys = (
        "vendor_task_slots",
        "vendor_stack_bytes_per_task",
    )
    for key in positive_keys:
        value = resource.get(key)
        if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
            raise ValueError(
                f"resource report {schema_revision} requires positive integer {key}"
            )
    worker_values: list[int] = []
    for key in ("worker_task_slots", "worker_stack_bytes_per_task"):
        value = resource.get(key)
        if resource["schema"] in {
            "hisi-rf-resource-report/v12",
            "hisi-rf-resource-report/v13",
            "hisi-rf-resource-report/v14",
        } and value is None:
            worker_values.append(0)
        elif isinstance(value, int) and not isinstance(value, bool) and value >= 0:
            worker_values.append(value)
        else:
            raise ValueError(
                f"resource report {schema_revision} requires a non-negative integer "
                f"or absent optional worker value for {key}"
            )
    worker_slots, worker_stack_bytes = worker_values
    if (worker_slots == 0) != (worker_stack_bytes == 0):
        raise ValueError(
            f"resource report {schema_revision} worker slots and stack bytes must "
            "both be absent/zero or both be positive"
        )

    coexistence_slots = 0
    coexistence_stacks = 0
    if resource["schema"] in {
        "hisi-rf-resource-report/v11",
        "hisi-rf-resource-report/v12",
        "hisi-rf-resource-report/v13",
        "hisi-rf-resource-report/v14",
    }:
        for key in ("coexistence_task_slots", "coexistence_stack_bytes"):
            value = resource.get(key)
            if not isinstance(value, int) or isinstance(value, bool) or value < 0:
                raise ValueError(
                    f"resource report {schema_revision} requires non-negative integer {key}"
                )
        coexistence_slots = resource["coexistence_task_slots"]
        coexistence_stacks = resource["coexistence_stack_bytes"]
        if (coexistence_slots == 0) != (coexistence_stacks == 0):
            raise ValueError(
                f"resource report {schema_revision} coexistence slots and stacks must both be zero "
                "or both be positive"
            )

    child_slots = (
        resource["vendor_task_slots"]
        + worker_slots
        + coexistence_slots
    )
    child_stacks = (
        resource["vendor_task_slots"] * resource["vendor_stack_bytes_per_task"]
        + worker_slots * worker_stack_bytes
        + coexistence_stacks
    )
    if resource["dynamic_tasks_required"] != child_slots:
        raise ValueError(
            f"resource report {schema_revision} task total does not equal child groups"
        )
    if resource["task_stack_bytes"] != child_stacks:
        raise ValueError(
            f"resource report {schema_revision} stack total does not equal child groups"
        )


def validate_l2_storage(resource: dict[str, Any]) -> None:
    if resource["schema"] != "hisi-rf-resource-report/v14":
        return
    l2 = resource.get("l2_storage")
    if not isinstance(l2, dict):
        raise ValueError("resource report v14 requires l2_storage")
    for key in ("rx_slots", "tx_slots", "mtu", "payload_bytes", "metadata_bytes", "total_bytes"):
        value = l2.get(key)
        if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
            raise ValueError(f"resource report v14 requires positive L2 {key}")
    offset = resource.get("l2_storage_offset")
    if not isinstance(offset, int) or isinstance(offset, bool) or offset < 0:
        raise ValueError("resource report v14 requires non-negative l2_storage_offset")
    if l2["payload_bytes"] != (l2["rx_slots"] + l2["tx_slots"]) * l2["mtu"]:
        raise ValueError("resource report v14 L2 payload does not equal slot capacity")
    if l2["total_bytes"] != l2["payload_bytes"] + l2["metadata_bytes"]:
        raise ValueError("resource report v14 L2 size does not equal its children")
    if offset + l2["total_bytes"] > resource["control_storage_bytes"]:
        raise ValueError("resource report v14 L2 storage exceeds control storage")


def validate_l2_layout(resource: dict[str, Any], parsed: ELFFile) -> None:
    """The v14 prototype requires its target descriptor, not a host size_of report."""
    if resource["schema"] != "hisi-rf-resource-report/v14":
        return
    if parsed.elfclass != 32 or not parsed.little_endian or parsed["e_machine"] != "EM_RISCV":
        raise ValueError("resource report v14 requires the RV32 target ELF")
    table = parsed.get_section_by_name(".symtab")
    def unique(name: str) -> Any:
        matches = table.get_symbol_by_name(name) if table else None
        if not matches or len(matches) != 1:
            raise ValueError(f"v14 target layout is missing or ambiguous: {name}")
        return matches[0]
    control = unique("NET0_CONTROL")
    layout = unique("NET0_STORAGE_LAYOUT")
    if control["st_size"] != resource["control_storage_bytes"] or layout["st_size"] != 52:
        raise ValueError("v14 control storage or descriptor size differs from target ELF")
    section = parsed.get_section(layout["st_shndx"])
    offset = layout["st_value"] - section["sh_addr"]
    data = section.data()
    if offset < 0 or offset + 52 > len(data):
        raise ValueError("v14 descriptor is outside its ELF section")
    l2 = resource["l2_storage"]
    expected = (
        int.from_bytes(b"NET0", "little"), 1,
        resource["control_storage_bytes"], resource["l2_storage_offset"],
        l2["total_bytes"], l2["payload_bytes"], l2["metadata_bytes"],
        l2["rx_slots"], l2["tx_slots"], l2["mtu"],
        resource["arena_storage_bytes"] + resource["runtime_arena_bytes"],
        resource["main_stack_bytes_required"], resource["linker_packet_ram_bytes"],
    )
    if struct.unpack("<13I", data[offset:offset + 52]) != expected:
        raise ValueError("v14 resource report differs from target-built layout descriptor")


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
        "hisi-rf-resource-report/v9",
        "hisi-rf-resource-report/v10",
        "hisi-rf-resource-report/v11",
        "hisi-rf-resource-report/v12",
        "hisi-rf-resource-report/v13",
        "hisi-rf-resource-report/v14",
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
        "hisi-rf-resource-report/v9",
        "hisi-rf-resource-report/v10",
        "hisi-rf-resource-report/v11",
        "hisi-rf-resource-report/v12",
        "hisi-rf-resource-report/v13",
        "hisi-rf-resource-report/v14",
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
        "hisi-rf-resource-report/v9",
        "hisi-rf-resource-report/v10",
        "hisi-rf-resource-report/v11",
        "hisi-rf-resource-report/v12",
        "hisi-rf-resource-report/v13",
        "hisi-rf-resource-report/v14",
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
    if resource["schema"] in {
        "hisi-rf-resource-report/v8",
        "hisi-rf-resource-report/v9",
        "hisi-rf-resource-report/v10",
        "hisi-rf-resource-report/v11",
        "hisi-rf-resource-report/v12",
        "hisi-rf-resource-report/v13",
        "hisi-rf-resource-report/v14",
    }:
        schema_revision = resource["schema"].rsplit("/", 1)[-1]
        for key in ("runtime_object_headroom_bytes", "runtime_arena_bytes"):
            value = resource.get(key)
            if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
                raise ValueError(
                    f"resource report {schema_revision} requires positive integer {key}"
                )
        if (
            resource["runtime_arena_bytes"]
            < resource["task_stack_bytes"] + resource["runtime_object_headroom_bytes"]
        ):
            raise ValueError(
                f"resource report {schema_revision} runtime_arena_bytes must cover "
                "task_stack_bytes "
                "+ runtime_object_headroom_bytes"
            )
        expected = (
            resource["control_storage_bytes"]
            + resource["arena_storage_bytes"]
            + resource["runtime_arena_bytes"]
        )
        if resource["caller_owned_bytes"] != expected:
            raise ValueError(
                f"resource report {schema_revision} caller_owned_bytes must equal "
                "control_storage_bytes "
                "+ arena_storage_bytes + runtime_arena_bytes"
            )
    if resource["schema"] in {
        "hisi-rf-resource-report/v9",
        "hisi-rf-resource-report/v10",
        "hisi-rf-resource-report/v11",
        "hisi-rf-resource-report/v12",
        "hisi-rf-resource-report/v13",
        "hisi-rf-resource-report/v14",
    }:
        minimum_stack = resource.get("minimum_task_stack_bytes")
        if (
            not isinstance(minimum_stack, int)
            or isinstance(minimum_stack, bool)
            or minimum_stack <= 0
        ):
            raise ValueError(
                "resource report v9+ requires positive integer minimum_task_stack_bytes"
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

    validate_task_resource_tree(resource)
    validate_l2_storage(resource)
    if resource["schema"] in {
        "hisi-rf-resource-report/v10",
        "hisi-rf-resource-report/v11",
        "hisi-rf-resource-report/v12",
        "hisi-rf-resource-report/v13",
        "hisi-rf-resource-report/v14",
    }:
        with elf.open("rb") as stream:
            parsed = ELFFile(stream)
            validate_l2_layout(resource, parsed)
            section = parsed.get_section_by_name(".hisi_shared_arenas")
            if section is None:
                raise ValueError("ELF is missing .hisi_shared_arenas")
            linked_shared_arena_bytes = int(section["sh_size"])
        expected_shared_arena_bytes = (
            resource["arena_storage_bytes"] + resource["runtime_arena_bytes"]
        )
        if linked_shared_arena_bytes != expected_shared_arena_bytes:
            raise ValueError(
                "ELF .hisi_shared_arenas size does not match resource report: "
                f"linked={linked_shared_arena_bytes}, expected={expected_shared_arena_bytes}"
            )

    resolved_resource = dict(resource)
    resolved_resource["flash_bytes"] = plan["image_len"]
    if resource["schema"] in {
        "hisi-rf-resource-report/v10",
        "hisi-rf-resource-report/v11",
        "hisi-rf-resource-report/v12",
        "hisi-rf-resource-report/v13",
        "hisi-rf-resource-report/v14",
    }:
        resolved_resource["linked_shared_arena_bytes"] = linked_shared_arena_bytes
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
                    "schema": "hisi-rf-resource-report/v9",
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
                    "minimum_task_stack_bytes": 24 * 1024,
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
        assert persisted["resource"]["minimum_task_stack_bytes"] == 24 * 1024
        assert persisted["resource"]["runtime_object_headroom_bytes"] == 16 * 1024
        assert persisted["resource"]["runtime_arena_bytes"] == 188_928
        assert persisted["resource"]["shared_rf_arena_bytes"] == 114_176
        assert persisted["resource"]["caller_owned_bytes"] == 311_776
        assert persisted["artifact"]["elf_name"] == elf.name
        assert persisted["artifact"]["image_name"] == image.name
        assert str(root) not in output.read_text(encoding="utf-8")

        resource_v11 = dict(persisted["resource"])
        resource_v11["schema"] = "hisi-rf-resource-report/v11"
        resource_v11.update(
            {
                "vendor_task_slots": 7,
                "vendor_stack_bytes_per_task": 24 * 1024,
                "worker_task_slots": 0,
                "worker_stack_bytes_per_task": 0,
                "coexistence_task_slots": 4,
                "coexistence_stack_bytes": 10_240,
                "dynamic_tasks_required": 11,
                "task_stack_bytes": 7 * 24 * 1024 + 10_240,
            }
        )
        validate_task_resource_tree(resource_v11)
        resource_v11["dynamic_tasks_required"] += 1
        try:
            validate_task_resource_tree(resource_v11)
        except ValueError as error:
            assert "task total" in str(error)
        else:
            raise AssertionError("inconsistent v11 coexistence task total was accepted")

        resource_v9 = load_object(resource_path, "resource report")
        resource_v8 = dict(resource_v9)
        resource_v8["schema"] = "hisi-rf-resource-report/v8"
        resource_v8.pop("minimum_task_stack_bytes")
        resource_path.write_text(json.dumps(resource_v8), encoding="utf-8")
        assert assemble(resource_path, plan_path, elf, image)["resource"]["schema"].endswith(
            "/v8"
        )

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

        resource_path.write_text(json.dumps(resource_v9), encoding="utf-8")
        plan["image_len"] += 1
        plan_path.write_text(json.dumps(plan), encoding="utf-8")
        try:
            assemble(resource_path, plan_path, elf, image)
        except ValueError as error:
            assert "image_len" in str(error)
        else:
            raise AssertionError("mismatched FlashPlan image_len was accepted")

        plan["image_len"] = image.stat().st_size
        plan_path.write_text(json.dumps(plan), encoding="utf-8")
        resource_v12 = dict(resource_v11)
        resource_v12["schema"] = "hisi-rf-resource-report/v12"
        resource_v12["worker_task_slots"] = None
        resource_v12["worker_stack_bytes_per_task"] = None
        resource_v12["dynamic_tasks_required"] = (
            resource_v12["vendor_task_slots"]
            + resource_v12["coexistence_task_slots"]
        )
        resource_v12["task_stack_bytes"] = (
            resource_v12["vendor_task_slots"]
            * resource_v12["vendor_stack_bytes_per_task"]
            + resource_v12["coexistence_stack_bytes"]
        )
        validate_task_resource_tree(resource_v12)

        resource_v13 = dict(resource_v12)
        resource_v13["schema"] = "hisi-rf-resource-report/v13"
        assert resource_v13["schema"] in RESOURCE_SCHEMAS
        validate_task_resource_tree(resource_v13)

        resource_v14 = dict(resource_v9)
        resource_v14.update(
            schema="hisi-rf-resource-report/v14",
            control_storage_bytes=21_376,
            l2_storage_offset=2_296,
            l2_storage={
                "rx_slots": 4, "tx_slots": 4, "mtu": 1_514,
                "payload_bytes": 12_112, "metadata_bytes": 448, "total_bytes": 12_560,
            },
        )
        resource_v14["caller_owned_bytes"] = (
            resource_v14["control_storage_bytes"]
            + resource_v14["arena_storage_bytes"]
            + resource_v14["runtime_arena_bytes"]
        )
        assert resource_v14["schema"] in RESOURCE_SCHEMAS
        validate_l2_storage(resource_v14)
        from unittest.mock import MagicMock
        target_words = [
            int.from_bytes(b"NET0", "little"), 1, 21_376, 2_296, 12_560, 12_112, 448,
            4, 4, 1_514, 114_240 + 188_928, 0x8000, 0x24000,
        ]
        symbols = {
            "NET0_CONTROL": {"st_size": 21_376},
            "NET0_STORAGE_LAYOUT": {"st_size": 52, "st_value": 0x230400, "st_shndx": 1},
        }
        symbol_table = MagicMock()
        symbol_table.get_symbol_by_name.side_effect = lambda name: [symbols[name]] if name in symbols else None
        descriptor = MagicMock()
        descriptor.__getitem__.return_value = 0x230400
        descriptor.data.return_value = struct.pack("<13I", *target_words)
        parsed = MagicMock(elfclass=32, little_endian=True)
        parsed.__getitem__.return_value = "EM_RISCV"
        parsed.get_section_by_name.return_value = symbol_table
        parsed.get_section.return_value = descriptor
        validate_l2_layout(resource_v14, parsed)
        for index in range(len(target_words)):
            changed = target_words.copy()
            changed[index] += 1
            descriptor.data.return_value = struct.pack("<13I", *changed)
            try:
                validate_l2_layout(resource_v14, parsed)
            except ValueError:
                pass
            else:
                raise AssertionError(f"ELF/report mismatch at descriptor word {index} accepted")
        descriptor.data.return_value = struct.pack("<13I", *target_words)
        symbol_table.get_symbol_by_name.side_effect = lambda _name: None
        try:
            validate_l2_layout(resource_v14, parsed)
        except ValueError as error:
            assert "missing or ambiguous" in str(error)
        else:
            raise AssertionError("v14 ELF without target layout proof was accepted")
        for key in resource_v14["l2_storage"]:
            for invalid in (None, True, -1, 0, "4"):
                bad = dict(resource_v14, l2_storage=dict(resource_v14["l2_storage"]))
                bad["l2_storage"][key] = invalid
                try:
                    validate_l2_storage(bad)
                except ValueError:
                    pass
                else:
                    raise AssertionError(f"invalid L2 {key}={invalid!r} was accepted")
        for change in (
            {"l2_storage": None},
            {"l2_storage_offset": -1},
            {"l2_storage_offset": True},
            {"l2_storage_offset": 21_376},
            {"l2_storage": dict(resource_v14["l2_storage"], total_bytes=12_561)},
            {"l2_storage": dict(resource_v14["l2_storage"], rx_slots=5)},
        ):
            try:
                validate_l2_storage(dict(resource_v14, **change))
            except ValueError:
                pass
            else:
                raise AssertionError(f"inconsistent L2 resource tree accepted: {change!r}")
        double_counted = dict(resource_v14)
        double_counted["caller_owned_bytes"] += double_counted["l2_storage"]["total_bytes"]
        resource_path.write_text(json.dumps(double_counted), encoding="utf-8")
        try:
            assemble(resource_path, plan_path, elf, image)
        except ValueError as error:
            assert "caller_owned_bytes" in str(error)
        else:
            raise AssertionError("L2 storage was counted twice")

        resource_v9["schema"] = "hisi-rf-resource-report/v15"
        resource_path.write_text(json.dumps(resource_v9), encoding="utf-8")
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
