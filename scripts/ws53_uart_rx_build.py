#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Build the independent WS53 Flash UART RX profile in a fresh Cargo directory.

Records the full resolved dependency trees, rust-src, Cargo configuration and
compiler identities. A receipt is local reproducibility evidence, not a signed
attestation or authorization to program hardware.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import elf_panic_sites

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = 'examples/ws53-flash-uart-rx/Cargo.toml'
SCHEMA = 'ws53-flash-uart-rx-build-v2'
CORE = ROOT/'scripts/ws53_ci_elf_build.py'
spec = importlib.util.spec_from_file_location('uart_rx_build_private_common', CORE)
common = importlib.util.module_from_spec(spec)
spec.loader.exec_module(common)
# Reuse the tracked source inventory in a private module instance. Panic
# locations use the explicit full-path DWARF validator, not its legacy policy.
common.MANIFEST = MANIFEST
common.FEATURES = []

SITES = {
    'independent-timer-timeout': ('examples/ws53-flash-uart-rx/src/main.rs', 'TIMER1 timeout'),
    'async-wake-timeout': ('examples/ws53-flash-uart-rx/src/uart_async.rs', 'UART2 async read did not wake'),
    'cancel-input-timeout': ('examples/ws53-flash-uart-rx/src/uart_async.rs', 'UART2 cancelled input timeout'),
}


def save(path, value):
    path.write_text(json.dumps(value, indent=2)+'\n')


def source_snapshot(root):
    result = common.source_snapshot(root, common.clean_environment())
    result['shared_flash_sources'] = {'examples/ws53-flash/src/capture_delay.rs':
        common.digest(root/'examples/ws53-flash/src/capture_delay.rs')}
    return result


def binding(elf, root):
    sites = {}
    for index, (name, (source, message)) in enumerate(SITES.items(), 1):
        lines = (root/source).read_text().splitlines()
        matches = [i for i, line in enumerate(lines, 1) if line.strip() == f'panic!("{message}");']
        common.require(len(matches) == 1, 'missing or ambiguous reviewed UART RX panic: '+name)
        sites[name] = (source, matches[0], index, message)
    hashes = {source:common.digest(root/source) for source, _ in SITES.values()}
    result = elf_panic_sites.validate(elf, root, sites, hashes)
    return dict(elf_sha256=result['elf_sha256'], source_sha256=result['source_sha256'],
                sites=result['sites'], dwarf_lines_verified=True, executed_on_hardware=False)


def command(root, output):
    return ['cargo', '+'+common.toolchain(root), 'build', '--locked', '--offline', '--release',
            '-Zbuild-std=core,alloc', '--manifest-path', MANIFEST, '--target', common.TARGET,
            '--target-dir', str(output/'cargo-target')]


def build(root, output):
    root, output = root.resolve(), output.resolve()
    common.require(not output.exists() and not output.is_relative_to(root/'examples'), 'fresh build directory required')
    output.mkdir(parents=True)
    before = source_snapshot(root)
    helpers = {str(p.relative_to(ROOT)):common.digest(p) for p in (Path(__file__), CORE, ROOT/'scripts/elf_panic_sites.py', ROOT/'scripts/check-ws53-flash-elf.py')}
    env = common.clean_environment()
    tools = {name:common.command_output([name, '+'+common.toolchain(root), '-Vv'], root, env)
             for name in ('cargo', 'rustc')}
    argv = command(root, output)
    with (output/'build.log').open('wb') as log:
        result = subprocess.run(argv, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
    common.require(result.returncode == 0, 'UART RX build failed; inspect build.log')
    common.require(source_snapshot(root) == before, 'UART RX build inputs changed')
    common.require(all(common.digest(ROOT/name) == sha for name, sha in helpers.items()), 'build helpers changed')
    elf = output/'firmware.elf'
    shutil.copyfile(output/'cargo-target'/common.TARGET/'release/ws53_flash_uart_rx', elf)
    layout, _ = common.load_validator().validate(elf)
    receipt = dict(schema=SCHEMA, build_succeeded=True, fresh_target=True,
        hardware_accessed=False, hardware_qualification_eligible=False,
        manifest=MANIFEST, features=[], target=common.TARGET, toolchain=common.toolchain(root),
        tools=tools, command=argv, working_directory=str(root), build_directory=str(output),
        source_inputs=before, inputs_unchanged=True, helper_sha256=helpers,
        environment_policy='minimal-offline-no-rustflags-or-wrappers',
        build_log_sha256=common.digest(output/'build.log'), elf_sha256=common.digest(elf),
        layout=layout, fault_binding=binding(elf, root))
    save(output/'receipt.json', receipt)
    return receipt


def validate(receipt_path, elf, root=ROOT):
    root, receipt_path, elf = root.resolve(), receipt_path.resolve(), elf.resolve()
    receipt = json.loads(receipt_path.read_text())
    for key, value in dict(schema=SCHEMA, build_succeeded=True, fresh_target=True,
        hardware_accessed=False, hardware_qualification_eligible=False, manifest=MANIFEST,
        features=[], target=common.TARGET, toolchain=common.toolchain(root), inputs_unchanged=True,
        environment_policy='minimal-offline-no-rustflags-or-wrappers').items():
        common.require(type(receipt.get(key)) is type(value) and receipt[key] == value, 'UART RX build receipt field changed: '+key)
    common.require(elf == receipt_path.parent/'firmware.elf' and receipt['build_directory'] == str(receipt_path.parent)
                   and receipt['working_directory'] == str(root) and receipt['command'] == command(root, receipt_path.parent),
                   'UART RX build receipt path or command mismatch')
    common.require(receipt['source_inputs'] == source_snapshot(root), 'UART RX build sources changed')
    common.require(receipt['helper_sha256'] == {str(p.relative_to(ROOT)):common.digest(p) for p in (Path(__file__), CORE, ROOT/'scripts/elf_panic_sites.py', ROOT/'scripts/check-ws53-flash-elf.py')},
                   'UART RX build helper changed')
    common.require(receipt['elf_sha256'] == common.digest(elf), 'UART RX ELF changed')
    common.require(receipt['build_log_sha256'] == common.digest(receipt_path.parent/'build.log'), 'UART RX build log changed')
    common.require(receipt['layout'] == common.load_validator().validate(elf)[0], 'UART RX ELF layout changed')
    common.require(receipt['fault_binding'] == binding(elf, root), 'UART RX fault attribution changed')
    env = common.clean_environment()
    common.require(receipt['tools'] == {name:common.command_output([name, '+'+common.toolchain(root), '-Vv'], root, env)
                                       for name in ('cargo', 'rustc')}, 'UART RX compiler identity changed')
    return dict(receipt_sha256=common.digest(receipt_path), elf_sha256=common.digest(elf),
                fault_binding=receipt['fault_binding'], source_inputs=receipt['source_inputs'])


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-root', type=Path, default=ROOT)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    receipt = build(args.source_root, args.output)
    print(json.dumps(dict(elf_sha256=receipt['elf_sha256'], passed=True)))
