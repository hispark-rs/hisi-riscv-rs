#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Fresh integrated WS53 IPC model; no peer firmware or hardware qualification."""
import argparse
import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CORE = ROOT/'scripts/ws53_ci_qemu_domain_build.py'
CORE_SHA = 'f9f853f420d59c0dfe3b7146fadf1abc2a18647d1377deb3dcd35daa2efd482f'
SCHEMA = 'ws53-ci-qemu-ipc-mask-build-v1'
OVERLAY_COMMIT = 'fa47993dae02a6e01ab0cab4e9d043592c11e3df'
SOURCE_INDEX_SHA = '0156f4fe45f944d8a62b6ff62d003d6ce720045ec2b34f999cee4b52fc52bba0'
spec = importlib.util.spec_from_file_location('ipc_mask_private_build_engine', CORE)
core = importlib.util.module_from_spec(spec); spec.loader.exec_module(core)
require, digest = core.require, core.digest
COPY_SOURCES = (
    'src/hw/riscv/ws63.c',
    'src/hw/riscv/hisi_riscv31.h',
    'src/hw/riscv/bs21.c',
    'src/hw/riscv/bs22.c',
    'src/hw/riscv/bs20.c',
    'src/hw/riscv/bs21e.c',
    'src/hw/riscv/ws53.c',
    'src/hw/riscv/ws53_flash_handoff.c.inc',
    'src/hw/riscv/ws53_cache_state.c.inc',
    'src/hw/riscv/ws53_instruction_cache.c.inc',
    'src/hw/riscv/ws53_sfc_command.c.inc',
    'src/hw/riscv/ws53_sdio.c.inc',
    'src/hw/riscv/ws53_sdio_adma.c.inc',
    'src/hw/riscv/ws53_uart.c.inc',
    'src/hw/riscv/ws53_uart_reset.c.inc',
    'src/hw/riscv/ws53_uart_route.c.inc',
    'src/hw/riscv/ws53_gpio.c.inc',
    'src/hw/riscv/ws53_pinctrl.c.inc',
    'src/hw/riscv/ws53_nmi.c.inc',
    'src/hw/riscv/ws53_wdt.c.inc',
    'src/hw/riscv/ws53_tcxo.c.inc',
    'src/hw/riscv/ws53_ipc.c.inc',
    'src/hw/riscv/ws53_rtc.c.inc',
    'src/hw/riscv/ws53_pwm.c.inc',
    'src/hw/riscv/ws53_i2s.c.inc',
    'src/hw/riscv/ws53_mclken0.c.inc',
    'src/hw/riscv/ws53_adc.c.inc',
    'src/hw/riscv/ws53_adc_capture.c.inc',
    'src/hw/riscv/ws53_mdma.c.inc',
    'src/hw/riscv/ws53_global_clken.c.inc',
    'src/hw/riscv/ws53_i2c.c.inc',
    'src/hw/riscv/ws53_mclken1.c.inc',
    'src/hw/riscv/ws53_spi.c.inc',
    'src/hw/riscv/ws53_tsensor.c.inc',
    'src/hw/riscv/ws53_trng.c.inc',
    'src/hw/riscv/ws53_hash.c.inc',
    'src/hw/riscv/ws53_km.c.inc',
    'src/hw/riscv/ws53_symc.c.inc',
    'src/hw/riscv/ws53_symc_block.h',
    'src/hw/riscv/ws53_hash_compress.h',
    'src/hw/riscv/ws53_efuse.c.inc',
    'src/target/riscv/insn_trans/trans_xlinx.c.inc',
    'src/tests/qtest/ws63-test.c',
)


def source_index(root):
    require(digest(CORE) == CORE_SHA, 'ipc build engine changed')
    require(core.git(root, 'rev-parse', 'HEAD').decode().strip() == OVERLAY_COMMIT,
        'ipc overlay commit differs')
    patches = sorted(str(p.relative_to(root)) for p in (root/'patches'/core.QEMU_TAG).glob('[0-9]*.patch'))
    require(len(COPY_SOURCES) == 43 and len(patches) == 11, '43 sources and eleven patches required')
    paths = ['scripts/build.sh', *COPY_SOURCES, *patches]
    require(not core.git(root, 'status', '--porcelain', '--untracked-files=all', '--', *paths),
        'ipc overlay build inputs dirty/untracked')
    result = {name:digest(core.regular(root, name)) for name in paths}
    require(core.canonical(result) == SOURCE_INDEX_SHA, 'ipc source/patch closure changed')
    return result


core.SCHEMA = SCHEMA
core.OVERLAY_COMMIT = OVERLAY_COMMIT
core.COPY_SOURCES = COPY_SOURCES
core.source_index = source_index


def validate_receipt(path, qemu, *, source_root, upstream_root=None):
    require(digest(CORE) == CORE_SHA, 'ipc build engine changed')
    receipt = json.loads(Path(path).read_text())
    require(receipt.get('policy_sha256') == digest(Path(__file__))
        and receipt.get('engine_sha256') == CORE_SHA, 'ipc policy/engine receipt changed')
    return core.validate_receipt(path, qemu, source_root=source_root, upstream_root=upstream_root) | dict(
        schema=SCHEMA, policy_sha256=digest(Path(__file__)), engine_sha256=CORE_SHA)


def build(args):
    before = digest(Path(__file__))
    receipt = core.build(args)
    out = args.output.resolve()
    try:
        require(digest(Path(__file__)) == before, 'ipc policy changed during build')
        receipt.update(policy_sha256=before, engine_sha256=CORE_SHA)
        pending = out/'ipc_mask-receipt.pending.json'
        pending.write_text(json.dumps(receipt, indent=2)+'\n')
        validate_receipt(pending, out/'qemu/build/qemu-system-riscv32', source_root=args.overlay_root)
        pending.replace(out/'receipt.json')
    except BaseException as error:
        receipt.update(passed=False, error=str(error), error_type=type(error).__name__)
        failed = json.dumps(receipt, indent=2)+'\n'
        (out/'receipt.json').write_text(failed)
        if (out/'ipc_mask-receipt.pending.json').exists():
            (out/'ipc_mask-receipt.pending.json').write_text(failed)
        raise
    return receipt


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--overlay-root', type=Path, required=True)
    parser.add_argument('--upstream-mirror', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--python', type=Path)
    parser.add_argument('--jobs', type=int, default=2)
    print(json.dumps(build(parser.parse_args()), indent=2))
