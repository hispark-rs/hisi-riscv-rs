#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Build a fresh, fixed-profile WS53 temperature ELF and record CI-only inputs.

The receipt binds a build performed here to its current source tree. It is
reproducibility evidence, not signed attestation or frozen hardware qualification.
No vendor data, programming tool or hardware transport is used.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tomllib

TARGET = 'riscv32imfc-unknown-none-elf'
FEATURES = ['capture-delay', 'tsensor-restart-probe']
MANIFEST = 'examples/ws53-flash/Cargo.toml'
FAULT_SOURCES = {
    'examples/ws53/ram_smoke/src/tsensor_probe.rs':
        'e9613a142ab02432a1998367ef0708112f07eb198c8c469da81bf3018a381f41',
    'examples/ws53-flash/src/tsensor_flash.rs':
        'e661e26409cc3ed707a0bd98c8a21b42ea1f7415eb3f7bbd81013aed8bf587c1',
}
PANIC_LINES = dict(zip(FAULT_SOURCES, (80, 119)))


def require(ok, message):
    if not ok:
        raise ValueError(message)


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def load_validator():
    spec = importlib.util.spec_from_file_location('ci_flash_validator', Path(__file__).with_name('check-ws53-flash-elf.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def clean_environment():
    # In particular do not inherit RUSTFLAGS, rustc wrappers or Cargo profile/
    # target overrides. Cargo's normal config files are hashed separately.
    names = ('PATH', 'HOME', 'RUSTUP_HOME', 'CARGO_HOME', 'TMPDIR', 'LANG', 'SSL_CERT_FILE')
    return {key: os.environ[key] for key in names if key in os.environ} | {'CARGO_NET_OFFLINE': 'true'}


def command_output(argv, root, env):
    result = subprocess.run(argv, cwd=root, env=env, capture_output=True, text=True, check=False)
    require(result.returncode == 0, 'build source/tool query failed: '+repr(argv)+'\n'+result.stderr)
    return result.stdout


def toolchain(root):
    return tomllib.loads((root/'rust-toolchain.toml').read_text())['toolchain']['channel']


def metadata(root, env):
    argv = ['cargo', '+'+toolchain(root), 'metadata', '--locked', '--offline', '--format-version', '1',
            '--manifest-path', MANIFEST, '--filter-platform', TARGET, '--features', ','.join(FEATURES)]
    return json.loads(command_output(argv, root, env))


def tree_files(folder):
    """Inventory all real dependency files, excluding build/VCS caches only."""
    result = {}
    for parent, dirs, files in os.walk(folder, followlinks=False):
        dirs[:] = sorted(name for name in dirs if name not in ('.git', 'target', '__pycache__', '.venv'))
        require(all(not (Path(parent)/name).is_symlink() for name in dirs),
                'symlink directory in recorded build input: '+str(parent))
        for name in sorted(files):
            path = Path(parent)/name
            if name == '.git':
                continue
            require(not path.is_symlink(), 'symlink in recorded build input: '+str(path))
            result[path.relative_to(folder).as_posix()] = digest(path)
    return result


def source_snapshot(root, env):
    data = metadata(root, env)
    reachable = {node['id'] for node in data['resolve']['nodes']}
    packages = []
    identities = {}
    for package in sorted(data['packages'], key=lambda p:p['id']):
        if package['id'] not in reachable:
            continue
        folder = Path(package['manifest_path']).parent.resolve()
        source = package['source']
        require(source is not None or folder.is_relative_to(root), 'path dependency outside source checkout')
        identities[package['id']] = ('local:'+folder.relative_to(root).as_posix() if source is None else
                                     source+':'+package['name']+'@'+package['version'])
        packages.append(dict(name=package['name'], version=package['version'], source=source,
            path=folder.relative_to(root).as_posix() if source is None else None,
            files=tree_files(folder)))
    # Shared #[path] fixtures live outside the Flash package and Cargo metadata.
    shared = tree_files(root/'examples/ws53/ram_smoke/src')
    explicit = {name:digest(root/name) for name in
        ('rust-toolchain.toml', '.cargo/config.toml', 'examples/ws53-flash/Cargo.lock')}
    sysroot = Path(command_output(['rustc', '+'+toolchain(root), '--print', 'sysroot'], root, env).strip())
    # build-std uses these source files rather than prebuilt target libraries.
    standard = tree_files(sysroot/'lib/rustlib/src/rust/library')
    require(standard and 'core/src/lib.rs' in standard and 'alloc/src/lib.rs' in standard,
            'pinned rust-src unavailable')
    configs = {}
    ancestors = [root, *root.parents]
    cargo_home = Path(env.get('CARGO_HOME', str(Path(env['HOME'])/'.cargo')))
    for folder in [*(path/'.cargo' for path in ancestors), cargo_home]:
        for name in ('config', 'config.toml'):
            path = folder/name
            if path.is_file():
                # Relative names where portable; external config remains an
                # explicit hashed input and may not silently differ across jobs.
                key = path.relative_to(root).as_posix() if path.is_relative_to(root) else str(path)
                configs[key] = digest(path)
    graph = dict(root=identities[data['resolve']['root']], nodes=sorted([
        dict(id=identities[node['id']], features=sorted(node['features']),
             dependencies=sorted(identities[value] for value in node['dependencies']),
             deps=[dict(dep, pkg=identities[dep['pkg']]) for dep in node['deps']])
        for node in data['resolve']['nodes']], key=lambda node:node['id']))
    return dict(packages=packages, shared_ram_fixture_sources=shared, explicit=explicit,
                rust_standard_library=standard, cargo_configs=configs,
                resolve=graph)


def commits(root):
    result = {}
    for name in ('.', 'crates/hisi-hal', 'crates/hisi-riscv-rt', 'crates/chips/ws53/ws53-pac'):
        folder = root/name
        value = subprocess.run(['git', '-C', str(folder), 'rev-parse', 'HEAD'], capture_output=True, text=True)
        require(value.returncode == 0, 'missing source revision: '+name)
        status = subprocess.run(['git', '-C', str(folder), 'status', '--porcelain', '--untracked-files=normal'],
                                capture_output=True, text=True, check=True)
        result[name] = dict(commit=value.stdout.strip(), dirty=bool(status.stdout.strip()))
    return result


def validate_fault_binding(elf, root):
    for name, expected in FAULT_SOURCES.items():
        require(digest(root/name) == expected, 'CI fault source changed; review required: '+name)
    validator = load_validator()
    observed = set()
    with elf.open('rb') as stream:
        image = validator.ELFFile(stream)
        require(image.has_dwarf_info(), 'CI fault attribution requires compiler debug line information')
        dwarf = image.get_dwarf_info()
        for cu in dwarf.iter_CUs():
            program = dwarf.line_program_for_CU(cu)
            if program is None:
                continue
            for entry in program.get_entries():
                row = entry.state
                if row is None or row.end_sequence:
                    continue
                offset = row.file-(0 if program.header.version >= 5 else 1)
                name = program.header.file_entry[offset].name.decode(errors='strict')
                for relative, line in PANIC_LINES.items():
                    if name.endswith('/'+Path(relative).name) or name == Path(relative).name:
                        if row.line == line:
                            observed.add(relative)
    require(observed == set(FAULT_SOURCES), 'compiled reviewed panic sites absent from ELF')
    return dict(source_sha256=dict(FAULT_SOURCES), panic_lines=dict(PANIC_LINES),
                dwarf_lines_verified=True, executed_elf_binding_verified=True)


def fault_binding(elf, root):
    # A normal source-regression build may be useful after fixture edits, but
    # its failure path cannot inherit the reviewed source/line attribution.
    current = {name:digest(root/name) for name in FAULT_SOURCES}
    if current != FAULT_SOURCES:
        return dict(source_sha256=current, dwarf_lines_verified=False, executed_elf_binding_verified=False,
                    review_required=True)
    return validate_fault_binding(elf, root)


def build_command(version, output):
    return ['cargo', '+'+version, 'build', '--locked', '--offline', '--release', '-Zbuild-std=core,alloc',
            '--manifest-path', MANIFEST, '--target', TARGET, '--target-dir', str(output/'cargo-target'),
            '--features', ','.join(FEATURES)]


def build(root, output):
    root, output = root.resolve(), output.resolve()
    require(not output.exists(), 'fresh build output must not exist')
    require(not output.is_relative_to(root/'examples'), 'build output cannot be a source directory')
    output.mkdir(parents=True)
    env = clean_environment()
    builder_hash = digest(Path(__file__))
    version = toolchain(root)
    before = source_snapshot(root, env)
    tools = {name:command_output([name, '+'+version, '-Vv'], root, env) for name in ('cargo', 'rustc')}
    command = build_command(version, output)
    with (output/'build.log').open('wb') as log:
        result = subprocess.run(command, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
    require(result.returncode == 0, 'fresh cargo build failed; retain build.log')
    after = source_snapshot(root, env)
    require(before == after and builder_hash == digest(Path(__file__)), 'build inputs changed during controlled build')
    produced = output/'cargo-target'/TARGET/'release/ws53_flash_smoke'
    elf = output/'firmware.elf'
    shutil.copyfile(produced, elf)
    layout, _ = load_validator().validate(elf)
    binding = fault_binding(elf, root)
    receipt = dict(schema=1, kind='ws53-tsensor-ci-elf-build', ci_only=True,
        hardware_qualification_eligible=False, build_succeeded=True, fresh_target=True,
        target=TARGET, features=FEATURES, manifest=MANIFEST, toolchain=version, tools=tools,
        command=command, working_directory=str(root), build_directory=str(output),
        environment_policy='minimal-offline-no-rustflags-or-wrappers',
        sources=before, inputs_unchanged=True, source_commits=commits(root),
        elf_sha256=digest(elf), layout=layout, fault_binding=binding,
        build_log_sha256=digest(output/'build.log'), builder_sha256=builder_hash)
    (output/'receipt.json').write_text(json.dumps(receipt, indent=2)+'\n')
    return receipt


def validate_receipt(receipt_path, elf, *, source_root):
    root, elf = Path(source_root).resolve(), Path(elf).resolve()
    receipt_path = Path(receipt_path)
    receipt = json.loads(receipt_path.read_text())
    expected = dict(schema=1, kind='ws53-tsensor-ci-elf-build', ci_only=True,
        hardware_qualification_eligible=False, build_succeeded=True, fresh_target=True,
        target=TARGET, features=FEATURES, manifest=MANIFEST, inputs_unchanged=True,
        environment_policy='minimal-offline-no-rustflags-or-wrappers')
    for key, value in expected.items():
        require(type(receipt.get(key)) is type(value) and receipt[key] == value, 'invalid CI ELF receipt field: '+key)
    require(receipt.get('builder_sha256') == digest(Path(__file__)), 'CI builder source changed')
    directory = receipt.get('build_directory')
    require(isinstance(directory, str) and Path(directory).is_absolute() and
            receipt.get('command') == build_command(toolchain(root), Path(directory)),
            'CI fixed cargo command changed')
    require(receipt.get('elf_sha256') == digest(elf), 'CI ELF differs from controlled build')
    layout, _ = load_validator().validate(elf)
    require(layout == receipt.get('layout'), 'CI ELF layout differs from build receipt')
    require(receipt.get('toolchain') == toolchain(root), 'CI toolchain pin changed')
    env = clean_environment()
    require(source_snapshot(root, env) == receipt.get('sources'), 'CI build dependency/source inputs changed')
    require({name:command_output([name, '+'+toolchain(root), '-Vv'], root, env) for name in ('cargo', 'rustc')} ==
            receipt.get('tools'), 'CI compiler/Cargo identity changed')
    binding = fault_binding(elf, root)
    require(binding == receipt.get('fault_binding'), 'CI reviewed fault binding changed')
    require((receipt_path.parent/'build.log').is_file() and
            digest(receipt_path.parent/'build.log') == receipt.get('build_log_sha256'), 'CI build log missing/changed')
    return dict(ci_only=True, hardware_qualification_eligible=False, receipt_sha256=digest(receipt_path),
        elf_sha256=digest(elf), fault_binding=binding, features=list(FEATURES), target=TARGET,
        sources_sha256=hashlib.sha256(json.dumps(receipt['sources'], sort_keys=True).encode()).hexdigest())


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = build(args.source_root, args.output)
    print(json.dumps({key:result[key] for key in ('ci_only', 'elf_sha256', 'features')}))
