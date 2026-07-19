#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Rebuild the SDK supplicant as a deterministic Personal-mode archive.

The SDK Ninja graph is the compiler-command oracle. This tool preserves its
compiler, ABI flags and include paths, but selects sources, feature defines and
the output archive through a checked profile. WPA2 remains the default profile.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import shlex
import subprocess
import sys
import tomllib
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path


TOOLS = Path(__file__).resolve().parent
CHECKER_PATH = TOOLS / "check-wpa-profile.py"
SPEC = importlib.util.spec_from_file_location("check_wpa_profile", CHECKER_PATH)
assert SPEC and SPEC.loader
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


def define_name(argument: str) -> str | None:
    if not argument.startswith("-D"):
        return None
    return argument[2:].split("=", 1)[0]


def source_from_command(command: list[str], sdk: Path) -> str | None:
    try:
        source = Path(command[command.index("-c") + 1]).resolve()
        return "/" + source.relative_to(sdk.resolve()).as_posix()
    except (ValueError, IndexError):
        return None


def replace_output(command: list[str], output: Path) -> list[str]:
    rewritten = command.copy()
    index = rewritten.index("-o") + 1
    rewritten[index] = str(output)
    return rewritten


def replace_source(command: list[str], source: Path) -> list[str]:
    rewritten = command.copy()
    index = rewritten.index("-c") + 1
    rewritten[index] = str(source)
    return rewritten


def strip_dependency_output(command: list[str]) -> list[str]:
    rewritten: list[str] = []
    skip_next = False
    for argument in command:
        if skip_next:
            skip_next = False
            continue
        if argument in ("-MF", "-MT", "-MQ"):
            skip_next = True
            continue
        if argument in ("-MD", "-MMD", "-MP"):
            continue
        rewritten.append(argument)
    return rewritten


def run_compile(command: list[str], source: str) -> tuple[str, str]:
    result = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if result.returncode:
        raise RuntimeError(f"compile failed: {source}\n{result.stdout}")
    return source, result.stdout


def remove_stale_archive(archive: Path) -> None:
    """Make source removal effective when reusing a build directory."""
    archive.unlink(missing_ok=True)


def profile_basename(
    profile: dict[str, object], field: str, default: str, suffix: str
) -> str:
    """Return a profile-controlled output basename without allowing path escape."""
    value = profile.get(field, default)
    if not isinstance(value, str) or Path(value).name != value or not value.endswith(suffix):
        raise ValueError(f"invalid profile {field}: {value}")
    return value


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sdk", type=Path, required=True)
    parser.add_argument(
        "--profile", type=Path, default=TOOLS / "wpa2-personal-profile.toml"
    )
    parser.add_argument("--build-dir", type=Path, required=True)
    parser.add_argument("--jobs", type=int, default=max(1, os.cpu_count() or 1))
    parser.add_argument("--print-commands", action="store_true")
    parser.add_argument(
        "--preserve-define",
        action="append",
        default=[],
        help="diagnostic override: keep one otherwise-forbidden SDK define",
    )
    args = parser.parse_args()

    sdk = args.sdk.resolve()
    profile = tomllib.loads(args.profile.read_text())
    try:
        archive_name = profile_basename(
            profile, "archive_name", "libwpa_supplicant_wpa2_personal.a", ".a"
        )
    except ValueError as error:
        print(error, file=sys.stderr)
        return 1
    component = sdk / profile["sdk_component"]
    cmake = component.read_text()
    all_sources = CHECKER.cmake_sources(CHECKER.cmake_block(cmake, "SOURCES"))
    fragments = tuple(profile["forbidden_source_fragments"])
    selected = {
        source for source in all_sources if not any(fragment in source for fragment in fragments)
    }

    ninja_dir = sdk / "output/ws63/acore/ws63-liteos-app"
    commands_text = subprocess.check_output(
        ["ninja", "-C", str(ninja_dir), "-t", "commands"], text=True
    )
    compile_commands: dict[str, list[str]] = {}
    archive_command: list[str] | None = None
    for line in commands_text.splitlines():
        if "libwpa_supplicant.a" in line and " rcD " in line:
            archive_command = shlex.split(line)
        if "CMakeFiles/wpa_supplicant.dir/" not in line or " -c " not in line:
            continue
        command = shlex.split(line)
        source = source_from_command(command, sdk)
        if source in selected:
            compile_commands[source] = command

    missing = sorted(selected - compile_commands.keys())
    if missing:
        print("selected sources absent from Ninja graph:\n" + "\n".join(missing), file=sys.stderr)
        return 1
    if archive_command is None:
        print("SDK Ninja graph has no libwpa_supplicant archive command", file=sys.stderr)
        return 1

    build_dir = args.build_dir.resolve()
    object_dir = build_dir / "obj"
    object_dir.mkdir(parents=True, exist_ok=True)
    forbidden = set(profile["forbidden_defines"]) - set(args.preserve_define)
    undef_header = build_dir / f"{profile['name']}-undef.h"
    undef_header.write_text(
        "/* Generated by build-wpa2-personal.py. */\n"
        + "".join(f"#undef {name}\n" for name in sorted(forbidden))
    )

    objects: list[Path] = []
    jobs: list[tuple[list[str], str]] = []
    for ordinal, source in enumerate(sorted(selected)):
        command = [arg for arg in compile_commands[source] if define_name(arg) not in forbidden]
        output = object_dir / f"{ordinal:03d}-{Path(source).stem}.o"
        command = strip_dependency_output(replace_output(command, output))
        command.extend(["-include", str(undef_header)])
        objects.append(output)
        jobs.append((command, source))
        if args.print_commands:
            print(shlex.join(command))

    template = next(iter(compile_commands.values()))
    for stub_name in profile.get("stub_sources", []):
        stub = TOOLS / stub_name
        if not stub.is_file():
            print(f"profile stub source does not exist: {stub}", file=sys.stderr)
            return 1
        output = object_dir / f"{len(objects):03d}-{stub.stem}.o"
        command = [arg for arg in template if define_name(arg) not in forbidden]
        command = strip_dependency_output(replace_output(replace_source(command, stub), output))
        command.extend(["-include", str(undef_header)])
        objects.append(output)
        jobs.append((command, f"profile:{stub_name}"))
        if args.print_commands:
            print(shlex.join(command))

    failures: list[str] = []
    with ThreadPoolExecutor(max_workers=args.jobs) as executor:
        futures = {executor.submit(run_compile, command, source): source for command, source in jobs}
        for future in as_completed(futures):
            try:
                future.result()
            except RuntimeError as error:
                failures.append(str(error))
    if failures:
        print("\n\n".join(failures), file=sys.stderr)
        return 1

    archive = build_dir / archive_name
    ar = archive_command[2] if archive_command[:2] == [":", "&&"] else archive_command[0]
    remove_stale_archive(archive)
    subprocess.run([ar, "rcD", str(archive), *(str(path) for path in objects)], check=True)
    manifest = {
        "profile": profile["name"],
        "archive": str(archive),
        "source_count": len(selected),
        "sources": sorted(selected),
        "stub_sources": profile.get("stub_sources", []),
        "forbidden_defines": sorted(forbidden),
        "preserved_defines": sorted(args.preserve_define),
    }
    (build_dir / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print(f"built {archive} from {len(selected)} sources")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
