#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Execute the actual Rust SDIO initialization HAL with a virtual event source.

This is a host-paced CPU fixture, not autonomous boot, full SDIO enumeration or
hardware qualification. Only a declared ordinary-RAM command mailbox is written.
No MMIO setup, CPU register changes, USB or Flash access is performed by this tool.
"""
import argparse
from contextlib import contextmanager
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import tempfile
import time
from elftools.elf.elffile import ELFFile
from ws53_qemu_fixture import connect, elf_check

NOTICE='ws53-sdio: assumed initialization/latched-status/ACK profile; no CIS, ADMA or host protocol'


def digest(path): return hashlib.sha256(path.read_bytes()).hexdigest()


@contextmanager
def running(args, name, address, *, words=13, additional_options=(), notice=NOTICE):
    log=[]
    folder=args.output/name
    folder.mkdir()
    with tempfile.TemporaryDirectory(prefix='ws53-sdio-hal-') as temp:
        temp=Path(temp)
        argv=[str(args.qemu.resolve()),'-M','ws53','-S','-display','none','-monitor','none','-serial','null',
              '-kernel',str(args.elf.resolve()),'-global','ws53-sdio.assume-init-profile=on',
              '-global','ws53-sdio.config-seed=0xa55a5aa5',
              '-global','ws53-mclken0.initial-value=0x5aa50000','-global','ws53-mclken1.initial-value=0xa55a0011',
              '-qtest',f'unix:{temp}/qt,server=on,wait=off','-qtest-log','/dev/null',
              '-qmp',f'unix:{temp}/qm,server=on,wait=off','-d','guest_errors',
              *additional_options]
        proc=subprocess.Popen(argv,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        try:
            with connect(temp/'qt',proc) as qs,qs.makefile('rwb',buffering=0) as qt,connect(temp/'qm',proc) as ms,ms.makefile('rwb',buffering=0) as qm:
                json.loads(qm.readline())
                def qmp(name,arguments=None):
                    request=dict(execute=name)
                    if arguments is not None: request['arguments']=arguments
                    qm.write(json.dumps(request).encode()+b'\n')
                    while True:
                        result=json.loads(qm.readline())
                        assert 'error' not in result,result
                        if 'return' in result:
                            log.append(dict(qmp=request,result=result['return']))
                            return result['return']
                def command(text):
                    qt.write(text.encode()+b'\n'); result=qt.readline().decode().split()
                    assert result and result[0]=='OK',(text,result)
                    log.append(dict(qtest=text,result=result[1:]))
                    return result[1:]
                def report():
                    raw=bytes.fromhex(command(f'read {address:#x} {words*4}')[0].removeprefix('0x'))
                    value=struct.unpack(f'<{words}I',raw)
                    assert value[12]==0,('Rust panic',value)
                    return value
                def wait(predicate):
                    deadline=time.monotonic()+5
                    while True:
                        value=report()
                        if predicate(value): return value
                        assert time.monotonic()<deadline,('fixture timeout',value)
                        time.sleep(.001)
                def invoke(op):
                    seq=report()[1]
                    # Only the declared test mailbox command word is written.
                    command(f'writel {address:#x} {op}')
                    qmp('cont')
                    wait(lambda v:v[1]==seq+1)
                    qmp('stop')
                    value=report()
                    assert value[0]==0 and value[1]==seq+1
                    return value
                qmp('qmp_capabilities')
                try:
                    qmp('cont'); wait(lambda v:v[2]!=0); qmp('stop')
                    assert report()[2]==1,report()
                    yield command,qmp,invoke,report
                finally:
                    qmp('quit')
            stdout,stderr=proc.communicate(timeout=5)
            (folder/'stdout.log').write_bytes(stdout); (folder/'stderr.log').write_bytes(stderr)
            assert proc.returncode==0 and not stdout
            assert stderr.decode().splitlines()==[notice],stderr.decode()
        finally:
            if proc.poll() is None:
                proc.terminate()
            # Preserve emulator diagnostics even when a case assertion raises
            # inside the yielded context, before normal success validation.
            stdout,stderr=proc.communicate(timeout=5)
            (folder/'stdout.log').write_bytes(stdout)
            (folder/'stderr.log').write_bytes(stderr)
            (folder/'operations.json').write_text(json.dumps(log,indent=2)+'\n')
            (folder/'command.json').write_text(json.dumps(argv,indent=2)+'\n')


def run(args):
    assert not args.output.exists()
    contract=elf_check.validate(args.elf)
    with args.elf.open('rb') as source:
        elf=ELFFile(source)
        symbols=elf.get_section_by_name('.symtab').get_symbol_by_name('WS53_SDIO_INIT_MODEL')
        assert len(symbols)==1 and symbols[0]['st_size']==52
        address=symbols[0]['st_value']
        assert address%4==0 and 0x20010000<=address<=0x20060000-52
    args.output.mkdir(parents=True)
    results=[]
    for case in ('ready-message','timeout-clock','timeout-enable','host-clock-lost',
                 'unknown-event','transfer-event','function-reset','voltage-switch','ready-readback','cancel'):
        with running(args,case,address) as (cmd,qmp,invoke,report):
            def signal(name,n,value=1): cmd(f'set_irq_in /machine/sdio {name} {n} {value}')
            def count(name): return qmp('qom-get',dict(path='/machine/sdio',property=name))
            if case=='timeout-clock':
                value=invoke(2); assert value[3:5]==(1,6) and value[11]==6
            else:
                assert invoke(1)[3:5]==(1,0)
                signal('host',0)
                assert invoke(1)[3:5]==(2,0)
                if case=='ready-message':
                    signal('inject',9)
                    qmp('qom-set',dict(path='/machine/sdio',property='host-message',value=0x12345678))
                    value=invoke(1)
                    assert value[3:10]==(2,0,4,0,0x200,1,0x12345678),value
                    signal('host',1)
                    value=invoke(2)
                    assert value[3:5]==(3,0) and value[7]==0x2000 and value[10:12]==(1,0),value
                    assert invoke(1)[3:5]==(3,0)
                elif case=='timeout-enable':
                    value=invoke(2); assert value[3:5]==(2,6) and value[11]==6
                elif case=='host-clock-lost':
                    signal('host',0,0); value=invoke(1); assert value[4]==3
                elif case=='ready-readback':
                    signal('host',1); signal('fault',0)
                    value=invoke(1); assert value[4]==5 and value[7]==0x2000
                elif case=='cancel':
                    value=invoke(3); assert value[4]==0
                else:
                    bit={'unknown-event':16,'transfer-event':4,'function-reset':8,'voltage-switch':33}[case]
                    signal('inject',bit)
                    value=invoke(1); assert value[4]==4,value
                    if bit==33: assert value[6]==2
                    else: assert value[7]==1<<bit
                    assert count('acknowledgements')==0
            if case!='ready-message':
                before=count('acknowledgements')
                assert invoke(1)[4]==8
                assert count('acknowledgements')==before
            results.append(dict(case=case,passed=True,final_report=list(report()),
                                acknowledgements=count('acknowledgements'),host_enables=count('host-enables'),
                                rejections=count('rejections')))
            assert results[-1]['rejections']==0
    result=dict(passed=True,cases=results,elf_contract=contract,elf_sha256=digest(args.elf),
                qemu_sha256=digest(args.qemu),script_sha256=digest(Path(__file__)),
                mailbox_address=address,host_paced_fixture=True,host_mmio_setup=False,
                hardware_accessed=False,complete_sdio_enumeration=False,adma_transfers=False)
    (args.output/'report.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for option in ('qemu','elf','output'): parser.add_argument('--'+option,type=Path,required=True)
    run(parser.parse_args())
