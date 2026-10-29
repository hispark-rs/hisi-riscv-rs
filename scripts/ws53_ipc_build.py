#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Fresh source-bound IPC CPU fixture; model-only, never hardware admission."""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
from ws53_qemu_fixture import elf_check

ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('ipc_build_common',ROOT/'scripts/ws53_ci_elf_build.py')
common=importlib.util.module_from_spec(spec);spec.loader.exec_module(common)
common.MANIFEST='examples/ws53-ipc/Cargo.toml';common.FEATURES=['model-handoff']
BIN='ipc-model'
SCHEMA='ws53-ipc-model-elf-v1'
HELPERS=('scripts/ws53_ipc_build.py','scripts/ws53_ci_elf_build.py','scripts/check-ws53-ram-elf.py','scripts/ws53_qemu_fixture.py')


def snapshot():
    data=common.source_snapshot(ROOT,common.clean_environment());data.pop('shared_ram_fixture_sources')
    data['explicit']={n:common.digest(ROOT/n) for n in ('rust-toolchain.toml','.cargo/config.toml','examples/ws53-ipc/Cargo.lock')}
    return data


def command(out):
    return ['cargo','+'+common.toolchain(ROOT),'build','--release','--locked','--offline','-Zbuild-std=core,alloc',
        '--message-format=json-render-diagnostics','--manifest-path',common.MANIFEST,'--features','model-handoff',
        '--bin',BIN,'--target',common.TARGET,'--target-dir',str(out/'cargo-target')]


def validate(receipt,elf):
    out=Path(receipt).resolve().parent;r=json.loads(Path(receipt).read_text())
    common.require(r.get('passed') is True and r.get('schema')==SCHEMA
        and r.get('hardware_qualification_eligible') is False,'IPC source receipt boundary')
    common.require(r['sources']==snapshot() and r['helpers']=={n:common.digest(ROOT/n) for n in HELPERS},'IPC build inputs changed')
    common.require(Path(elf).resolve()==out/'firmware.elf' and r['elf_sha256']==common.digest(elf)
        and r['command']==command(out) and r['build_log_sha256']==common.digest(out/'build.log'),'IPC build bytes/path/command')
    common.require(r['layout']==elf_check.validate(elf),'IPC ELF contract changed')
    common.require(r['device_pacs']==['ws53_pac'] and r['tools']=={
        n:common.command_output([n,'+'+common.toolchain(ROOT),'-Vv'],ROOT,common.clean_environment())
        for n in ('cargo','rustc')},'IPC PAC/toolchain receipt changed')
    return r


def build(out):
    out.mkdir(parents=True,exist_ok=False);before=snapshot();helpers={n:common.digest(ROOT/n) for n in HELPERS}
    import subprocess
    with (out/'build.log').open('wb') as log:
        result=subprocess.run(command(out),cwd=ROOT,env=common.clean_environment(),stdout=log,stderr=subprocess.STDOUT)
    common.require(result.returncode==0,'IPC build failed; retain log')
    common.require(before==snapshot() and helpers=={n:common.digest(ROOT/n) for n in HELPERS},'IPC build inputs changed')
    elf=out/'firmware.elf';shutil.copyfile(out/'cargo-target'/common.TARGET/'release'/BIN,elf)
    artifacts=[json.loads(n) for n in (out/'build.log').read_text().splitlines() if n.startswith('{')]
    pacs={n['target']['name'] for n in artifacts if n.get('reason')=='compiler-artifact'
        and n['target']['name'].endswith('_pac') and n['target']['name']!='riscv_pac'}
    common.require(pacs=={'ws53_pac'},'IPC fixture requires only WS53 PAC')
    r=dict(passed=True,schema=SCHEMA,hardware_qualification_eligible=False,
        sources=before,helpers=helpers,command=command(out),device_pacs=sorted(pacs),
        elf_sha256=common.digest(elf),layout=elf_check.validate(elf),build_log_sha256=common.digest(out/'build.log'),
        tools={n:common.command_output([n,'+'+common.toolchain(ROOT),'-Vv'],ROOT,common.clean_environment()) for n in ('cargo','rustc')})
    (out/'receipt.json').write_text(json.dumps(r,indent=2)+'\n');validate(out/'receipt.json',elf)
    print(json.dumps(dict(passed=True,elf_sha256=r['elf_sha256'])))


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--output',type=Path,required=True);build(p.parse_args().output.resolve())
