#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Build the source-pinned WS53 control-domain QEMU with an independent CI receipt.

Receipts bind files produced by a trusted CI build; they are not signatures or
proof against a hostile host. They never grant any frozen candidate admission.
This independent policy preserves the original ws53_ci_qemu_build.py unchanged.
No existing QEMU source/build directory is reused or changed by this builder.
"""
import argparse
import configparser
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import tempfile
import time

SCHEMA = 'ws53-ci-qemu-domain-build-v1'
UPSTREAM_COMMIT = '7c949c53e936aa3a658d84ab53bae5cadaa5d59c'
UPSTREAM_URL = 'https://gitlab.com/qemu-project/qemu.git'
QEMU_TAG = 'v10.0.0'
# Reviewed overlay and exact build.sh + 31 copied sources + nine-patch closure.
# No caller-supplied hash or receipt can replace this policy; empty fails closed.
OVERLAY_COMMIT = '002d81fb1e0ada1643de35a555b8358aa9639df4'
SOURCE_INDEX_SHA256 = '64ba19da20a643c0e3be627c259b1fde96fb433b98713d2e94129c8fd37f19b4'
COPY_SOURCES = tuple('src/'+path for path in (
    'hw/riscv/ws63.c', 'hw/riscv/hisi_riscv31.h', 'hw/riscv/bs21.c',
    'hw/riscv/bs22.c', 'hw/riscv/bs20.c', 'hw/riscv/bs21e.c', 'hw/riscv/ws53.c',
    'hw/riscv/ws53_flash_handoff.c.inc', 'hw/riscv/ws53_uart.c.inc',
    'hw/riscv/ws53_uart_reset.c.inc', 'hw/riscv/ws53_gpio.c.inc',
    'hw/riscv/ws53_pinctrl.c.inc', 'hw/riscv/ws53_nmi.c.inc',
    'hw/riscv/ws53_wdt.c.inc', 'hw/riscv/ws53_tcxo.c.inc',
    'hw/riscv/ws53_rtc.c.inc', 'hw/riscv/ws53_pwm.c.inc',
    'hw/riscv/ws53_i2s.c.inc', 'hw/riscv/ws53_mclken0.c.inc',
    'hw/riscv/ws53_adc.c.inc', 'hw/riscv/ws53_adc_capture.c.inc',
    'hw/riscv/ws53_mdma.c.inc', 'hw/riscv/ws53_global_clken.c.inc',
    'hw/riscv/ws53_i2c.c.inc', 'hw/riscv/ws53_mclken1.c.inc',
    'hw/riscv/ws53_spi.c.inc', 'hw/riscv/ws53_tsensor.c.inc',
    'hw/riscv/ws53_trng.c.inc', 'hw/riscv/ws53_efuse.c.inc',
    'target/riscv/insn_trans/trans_xlinx.c.inc', 'tests/qtest/ws63-test.c'))
CONFIG_FILES = ('build/config-host.mak', 'build/build.ninja',
                'build/meson-info/intro-buildoptions.json',
                'build/meson-info/intro-compilers.json', 'build/meson-info/intro-dependencies.json')


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def canonical(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def git(root, *args, env=None):
    return subprocess.check_output(['git', '-c', 'core.excludesFile=/dev/null', '-c', 'core.hooksPath=/dev/null', '-C', str(root), *args],
                                   env=env, stderr=subprocess.PIPE)


def regular(root, relative):
    path = root/relative
    require(not Path(relative).is_absolute() and '..' not in Path(relative).parts
            and path.is_file() and not path.is_symlink() and root in path.resolve().parents,
            'missing/unsafe build input: '+relative)
    return path


def source_index(root):
    require(isinstance(OVERLAY_COMMIT, str) and re.fullmatch('[0-9a-f]{40}', OVERLAY_COMMIT)
            and isinstance(SOURCE_INDEX_SHA256, str) and re.fullmatch('[0-9a-f]{64}', SOURCE_INDEX_SHA256),
            'reviewed overlay policy is not frozen yet')
    require(git(root, 'rev-parse', 'HEAD').decode().strip() == OVERLAY_COMMIT, 'overlay commit differs from policy')
    patches = sorted(str(p.relative_to(root)) for p in (root/'patches'/QEMU_TAG).glob('[0-9]*.patch'))
    require(len(COPY_SOURCES) == 31 and len(patches) == 9, '31 copy inputs and nine patches required')
    paths = ['scripts/build.sh', *COPY_SOURCES, *patches]
    require(not git(root, 'status', '--porcelain', '--untracked-files=all', '--', *paths),
            'reviewed overlay build inputs are dirty/untracked')
    result = {name: digest(regular(root, name)) for name in paths}
    require(canonical(result) == SOURCE_INDEX_SHA256, 'overlay source/patch list differs from reviewed policy')
    return result


def expected_diff(tree, overlay):
    """Apply approved patches to a temporary Git index, never to the live tree."""
    objects = Path(git(tree, 'rev-parse', '--git-path', 'objects').decode().strip())
    objects = objects if objects.is_absolute() else tree/objects
    with tempfile.TemporaryDirectory(prefix='ws53-patch-index-') as temporary:
        folder = Path(temporary)
        subprocess.run(['git', 'init', '--bare', '-q', str(folder/'repo')], check=True)
        env = os.environ | {'GIT_DIR': str(folder/'repo'), 'GIT_INDEX_FILE': str(folder/'index'),
                            'GIT_ALTERNATE_OBJECT_DIRECTORIES': str(objects.resolve())}
        def command(*args):
            return subprocess.check_output(['git', *args], env=env, stderr=subprocess.PIPE)
        command('read-tree', UPSTREAM_COMMIT)
        for patch in sorted((overlay/'patches'/QEMU_TAG).glob('[0-9]*.patch')):
            command('apply', '--cached', str(patch))
        return command('diff', '--cached', '--binary', '--no-ext-diff', UPSTREAM_COMMIT)


def effective_sources(tree, overlay, approved):
    require(git(tree, 'rev-parse', 'HEAD').decode().strip() == UPSTREAM_COMMIT, 'QEMU upstream commit changed')
    actual = git(tree, 'diff', '--binary', '--no-ext-diff', 'HEAD')
    require(actual == expected_diff(tree, overlay), 'effective upstream files do not match all nine approved patches')
    copies = {name[4:]: digest(regular(tree, name[4:])) for name in COPY_SOURCES}
    require(all(copies[name[4:]] == approved[name] for name in COPY_SOURCES), 'copied model source differs from overlay')
    untracked = set(git(tree, 'ls-files', '--others', '--exclude-standard', '-z').decode().strip('\0').split('\0'))-{''}
    require(untracked == set(copies), 'unexpected/missing untracked QEMU source files')
    return dict(upstream_commit=UPSTREAM_COMMIT, patched_diff_sha256=hashlib.sha256(actual).hexdigest(), copies=copies)


def wrap_specs(tree):
    specs = {}
    for path in sorted((tree/'subprojects').glob('*.wrap')):
        parser = configparser.ConfigParser(interpolation=None)
        parser.read(path)
        if 'wrap-git' in parser:
            data = parser['wrap-git']
            revision = data['revision']
            require(re.fullmatch('[0-9a-f]{40}', revision), 'wrap revision must be a full pinned commit')
            specs[path.stem] = dict(revision=revision, url=data['url'],
                                   patch_directory=data.get('patch_directory'))
    return specs


def packagefile_sources(tree, spec):
    """Only tracked files from the pinned upstream packagefiles directory."""
    directory = spec.get('patch_directory')
    if directory is None:
        return {}
    require(isinstance(directory, str) and directory not in ('', '.')
            and not Path(directory).is_absolute() and '..' not in Path(directory).parts,
            'unsafe wrap patch_directory')
    prefix = 'subprojects/packagefiles/'+directory+'/'
    names = git(tree, 'ls-tree', '-r', '--name-only', '-z', UPSTREAM_COMMIT, '--', prefix).decode().split('\0')
    names = sorted(set(names)-{''})
    require(names, 'wrap packagefiles directory has no pinned tracked inputs: '+directory)
    result = {}
    for name in names:
        require(name.startswith(prefix), 'packagefile escaped reviewed directory')
        source = regular(tree, name)
        expected = hashlib.sha256(git(tree, 'show', UPSTREAM_COMMIT+':'+name)).hexdigest()
        require(digest(source) == expected, 'upstream packagefile differs from pinned commit: '+name)
        result[name[len(prefix):]] = dict(source=name, sha256=expected)
    return result


def apply_packagefiles(tree, folder, spec):
    files = packagefile_sources(tree, spec)
    tracked = set(git(folder, 'ls-files', '-z').decode().split('\0'))-{''}
    for relative, info in files.items():
        require(relative not in tracked, 'packagefiles may not overwrite tracked wrap source: '+relative)
        destination = folder/relative
        require(not destination.is_symlink() and folder in destination.resolve().parents,
                'unsafe wrap packagefile destination: '+relative)
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(regular(tree, info['source']), destination)
    return files


def wrap_state(tree):
    states = {}
    # This build profile uses pinned Git wraps. Do not silently admit an
    # instantiated archive wrap whose extracted inputs we have not qualified.
    for path in (tree/'subprojects').glob('*.wrap'):
        parser = configparser.ConfigParser(interpolation=None)
        parser.read(path)
        if 'wrap-file' in parser:
            directory = parser['wrap-file'].get('directory', path.stem)
            require(not (tree/'subprojects'/directory).exists(),
                    'active archive wrap requires a reviewed source verifier: '+path.stem)
    for name, spec in wrap_specs(tree).items():
        folder = tree/'subprojects'/name
        if not folder.exists():
            continue
        require((folder/'.git').exists(), 'active wrap has no pinned Git metadata: '+name)
        require(git(folder, 'rev-parse', 'HEAD').decode().strip() == spec['revision'], 'wrap commit mismatch: '+name)
        require(not git(folder, 'diff', 'HEAD', '--'), 'wrap tracked sources changed: '+name)
        packagefiles = packagefile_sources(tree, spec)
        for relative, info in packagefiles.items():
            require(digest(regular(folder, relative)) == info['sha256'],
                    'wrap packagefile differs from pinned source: '+name+'/'+relative)
        untracked = set(git(folder, 'ls-files', '--others', '--exclude-standard', '-z').decode().split('\0'))-{''}
        marker = '.meson-subproject-wrap-hash.txt'
        require(untracked <= set(packagefiles)|{marker}, 'unexpected untracked wrap source: '+name)
        if marker in untracked:
            require(regular(folder, marker).read_text().strip() == digest(tree/'subprojects'/(name+'.wrap')),
                    'Meson wrap marker differs from pinned declaration: '+name)
        # Packagefiles are copied from the pinned upstream tree, never from a
        # mirror's untracked files. Meson may also add its declaration marker.
        names = git(folder, 'ls-files', '-z').decode().split('\0')
        names += list(untracked)
        files = {relative: digest(regular(folder, relative)) for relative in sorted(set(names)-{''})}
        states[name] = spec | dict(files=files, packagefiles=packagefiles)
    return states


def tool_versions(env):
    versions = {}
    for name, argv in (
        ('cc', ['cc', '--version']), ('cxx', ['c++', '--version']), ('ld', ['ld', '--version']),
        ('make', ['make', '--version']), ('ninja', ['ninja', '--version']),
        ('python', [env.get('PYTHON', 'python3'), '--version']),
        ('glib', ['pkg-config', '--modversion', 'glib-2.0']),
        ('pixman', ['pkg-config', '--modversion', 'pixman-1']),
        ('slirp', ['pkg-config', '--modversion', 'slirp']),
    ):
        result = subprocess.run(argv, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=30)
        versions[name] = dict(argv=argv, exit_code=result.returncode,
            executable=shutil.which(argv[0], path=env.get('PATH')), output=result.stdout.decode(errors='replace'))
    return versions


def run_logged(command, cwd, env, out, records, *, timeout=3600):
    number = len(records)
    log = out/f'command-{number:03d}.log'
    row = dict(argv=list(map(str, command)), cwd=str(cwd), log=log.name,
               started_monotonic=time.monotonic(), exit_code=None)
    records.append(row)
    with log.open('wb') as stream:
        process = subprocess.Popen(row['argv'], cwd=cwd, env=env, stdout=stream, stderr=subprocess.STDOUT,
                                   stdin=subprocess.DEVNULL, start_new_session=True)
        try:
            row['exit_code'] = process.wait(timeout=timeout)
        except BaseException:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=5)
            raise
        finally:
            row['ended_monotonic'] = time.monotonic()
    row['log_sha256'] = digest(log)
    require(row['exit_code'] == 0, 'build command failed; see '+str(log))


def validate_receipt(receipt_path, qemu, *, source_root, upstream_root=None):
    path = Path(receipt_path).resolve()
    out = path.parent
    overlay = Path(source_root).resolve()
    receipt = json.loads(path.read_text())
    require(receipt.get('schema') == SCHEMA and receipt.get('passed') is True
            and receipt.get('ci_only') is True and receipt.get('hardware_qualification_eligible') is False
            and receipt.get('fresh_build') is True and receipt.get('model_tests_executed') is False,
            'not a complete CI-only fresh-build receipt')
    require(receipt.get('producer_sha256') == digest(Path(__file__)), 'model receipt producer changed')
    require(receipt.get('overlay_commit') == OVERLAY_COMMIT and receipt.get('upstream_commit') == UPSTREAM_COMMIT,
            'receipt commit differs from policy')
    approved = source_index(overlay)
    require(receipt.get('source_before') == approved == receipt.get('source_after'), 'overlay changed during/after build')
    tree = out/'qemu'
    require(tree.is_dir() and not tree.is_symlink() and tree.resolve().parent == out,
            'fresh QEMU tree must remain inside receipt directory')
    if upstream_root is not None:
        require(Path(upstream_root).resolve() == tree.resolve(), 'different upstream tree supplied')
    binary = regular(tree, 'build/qemu-system-riscv32')
    require(Path(qemu).resolve() == binary.resolve() and receipt.get('binary_sha256') == digest(binary),
            'executed QEMU binary differs from fresh receipt')
    require(receipt.get('sources_prepared') == effective_sources(tree, overlay, approved) == receipt.get('sources_built'),
            'effective source changed during/after build')
    require(receipt.get('wraps_built') == wrap_state(tree), 'wrap sources changed after build')
    require(receipt.get('config_sha256') == {name: digest(regular(tree, name)) for name in CONFIG_FILES},
            'QEMU configure/build configuration changed')
    require('TARGET_DIRS=riscv32-softmmu\n' in (tree/'build/config-host.mak').read_text(),
            'QEMU target configuration changed')
    require(isinstance(receipt.get('tool_versions'), dict) and set(receipt['tool_versions']) ==
            {'cc', 'cxx', 'ld', 'make', 'ninja', 'python', 'glib', 'pixman', 'slirp'},
            'compiler/runtime dependency versions absent')
    records = receipt.get('commands')
    require(isinstance(records, list) and len(records) >= 3 and all(isinstance(row, dict) and type(row.get('exit_code')) is int
            and row['exit_code'] == 0 for row in records), 'build command execution absent/failed')
    for row in records:
        require(isinstance(row, dict) and isinstance(row.get('argv'), list)
                and isinstance(row.get('log'), str)
                and type(row.get('started_monotonic')) in (int, float)
                and type(row.get('ended_monotonic')) in (int, float)
                and 0 <= row['started_monotonic'] <= row['ended_monotonic'],
                'invalid command record')
        require(row.get('log_sha256') == digest(regular(out, row['log'])), 'build command log changed')
    expected_build = ['bash', str(overlay/'scripts/build.sh')]
    require(sum(row.get('argv') == expected_build for row in records) == 1, 'exact approved build command missing/duplicated')
    env = receipt.get('build_environment', {})
    require(env.get('QEMU_TAG') == QEMU_TAG and env.get('QEMU_DIR') == str(tree)
            and env.get('CCACHE_DISABLE') == '1'
            and type(receipt.get('jobs')) is int and 1 <= receipt['jobs'] <= 256
            and env.get('JOBS') == str(receipt['jobs']), 'build environment/profile changed')
    return dict(passed=True, ci_only=True, hardware_qualification_eligible=False,
        model_sha256=digest(binary), source_sha256=approved, overlay_commit=OVERLAY_COMMIT,
        upstream_commit=UPSTREAM_COMMIT, receipt_sha256=digest(path),
        trusted_build_receipt=True, cryptographic_attestation=False,
        model_tests_executed=False)


def build(args):
    overlay = args.overlay_root.resolve()
    approved = source_index(overlay)
    out = args.output.resolve()
    require(not out.exists() and out != overlay and overlay not in out.parents, 'fresh output must not exist or be in overlay')
    require(type(args.jobs) is int and 1 <= args.jobs <= 256, 'invalid build parallelism')
    out.mkdir(parents=True)
    tree = out/'qemu'
    temporary = out/'temporary'
    temporary.mkdir()
    env = os.environ.copy()
    for key in list(env):
        if key.startswith('GIT_'):
            env.pop(key)
    for key in ('QEMU_DIR', 'QEMU_TAG', 'QEMU_REPO', 'CFLAGS', 'CXXFLAGS', 'LDFLAGS', 'GIT_DIR',
                'GIT_WORK_TREE', 'GIT_INDEX_FILE', 'GIT_ALTERNATE_OBJECT_DIRECTORIES', 'CC', 'CXX', 'AR', 'LD',
                'PYTHONPATH', 'PYTHONHOME', 'BASH_ENV', 'ENV', 'MAKEFLAGS', 'CONFIG_SITE', 'CPATH',
                'C_INCLUDE_PATH', 'CPLUS_INCLUDE_PATH', 'LIBRARY_PATH'):
        env.pop(key, None)
    env.update(QEMU_DIR=str(tree), QEMU_TAG=QEMU_TAG, JOBS=str(args.jobs), TMPDIR=str(temporary),
               LC_ALL='C', PYTHONHASHSEED='0', CC='cc', CXX='c++', CCACHE_DISABLE='1')
    if args.python is not None:
        env['PYTHON'] = str(args.python.resolve())
    records = []
    receipt = dict(schema=SCHEMA, passed=False, ci_only=True, hardware_qualification_eligible=False,
        fresh_build=True, model_tests_executed=False, producer_sha256=digest(Path(__file__)),
        overlay_commit=OVERLAY_COMMIT, upstream_commit=UPSTREAM_COMMIT, source_before=approved,
        jobs=args.jobs, commands=records, build_environment={key: env[key] for key in
            ('QEMU_DIR', 'QEMU_TAG', 'JOBS', 'TMPDIR', 'LC_ALL', 'PYTHONHASHSEED', 'PYTHON', 'CC', 'CXX',
             'PATH', 'PKG_CONFIG_PATH', 'PKG_CONFIG_LIBDIR', 'CCACHE_DISABLE') if key in env},
        trust_boundary='Trusted CI build receipt, not a signature or hostile-host proof')
    def run(command, cwd=out):
        run_logged(command, cwd, env, out, records)
    try:
        receipt['tool_versions'] = tool_versions(env)
        if args.upstream_mirror is not None:
            mirror = args.upstream_mirror.resolve()
            run(['git', 'clone', '--no-hardlinks', '--no-checkout', str(mirror), str(tree)])
        else:
            mirror = None
            run(['git', 'clone', '--depth', '1', '--branch', QEMU_TAG, UPSTREAM_URL, str(tree)])
        run(['git', '-C', str(tree), 'checkout', '--detach', UPSTREAM_COMMIT])
        require(git(tree, 'rev-parse', 'HEAD').decode().strip() == UPSTREAM_COMMIT, 'fresh clone is not the pinned base')
        require(not (tree/'build').exists(), 'source clone unexpectedly contains a build directory')
        receipt['mirror_wrap_inputs'] = {}
        if mirror is not None:
            for name, spec in wrap_specs(tree).items():
                source = mirror/'subprojects'/name
                if not (source/'.git').exists():
                    continue
                require(git(source, 'rev-parse', 'HEAD').decode().strip() == spec['revision'], 'mirror wrap HEAD differs: '+name)
                require(not git(source, 'diff', 'HEAD', '--') and not git(source, 'diff', '--cached', '--'),
                        'mirror wrap has tracked modifications: '+name)
                receipt['mirror_wrap_inputs'][name] = dict(revision=spec['revision'],
                    ignored_untracked=git(source, 'ls-files', '--others', '--exclude-standard').decode().splitlines())
                destination = tree/'subprojects'/name
                run(['git', 'clone', '--no-hardlinks', '--no-checkout', str(source), str(destination)])
                run(['git', '-C', str(destination), 'checkout', '--detach', spec['revision']])
                receipt['mirror_wrap_inputs'][name]['packagefiles'] = apply_packagefiles(tree, destination, spec)
        for name in COPY_SOURCES:
            destination = tree/name[4:]
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(regular(overlay, name), destination)
        for patch in sorted((overlay/'patches'/QEMU_TAG).glob('[0-9]*.patch')):
            run(['git', '-C', str(tree), 'apply', str(patch)])
        receipt['sources_prepared'] = effective_sources(tree, overlay, approved)
        run(['bash', str(overlay/'scripts/build.sh')], overlay)
        receipt['source_after'] = source_index(overlay)
        receipt['sources_built'] = effective_sources(tree, overlay, approved)
        require(receipt['sources_prepared'] == receipt['sources_built'], 'source changed during QEMU build')
        receipt['wraps_built'] = wrap_state(tree)
        receipt['config_sha256'] = {name: digest(regular(tree, name)) for name in CONFIG_FILES}
        receipt['binary_sha256'] = digest(regular(tree, 'build/qemu-system-riscv32'))
        receipt['passed'] = True
        candidate = out/'receipt.pending.json'
        candidate.write_text(json.dumps(receipt, indent=2)+'\n')
        validate_receipt(candidate, tree/'build/qemu-system-riscv32', source_root=overlay)
        candidate.replace(out/'receipt.json')
    except BaseException as error:
        receipt.update(passed=False, error=str(error), error_type=type(error).__name__)
        raise
    finally:
        if not receipt['passed']:
            failed = json.dumps(receipt, indent=2)+'\n'
            (out/'receipt.json').write_text(failed)
            # A failed post-build verification must not leave a temporary
            # successful receipt that a later consumer could select.
            if (out/'receipt.pending.json').exists():
                (out/'receipt.pending.json').write_text(failed)
    return receipt


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--overlay-root', type=Path, required=True)
    parser.add_argument('--upstream-mirror', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--python', type=Path)
    parser.add_argument('--jobs', type=int, default=2)
    print(json.dumps(build(parser.parse_args()), indent=2))
