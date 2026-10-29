#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Run real WS53 Rust KM HAL instructions against an explicit QEMU profile.

The host only writes the fixture mailbox, never KM MMIO, keys or loader state.
QOM/GPIO faults describe virtual hardware. No silicon handoff/erasure is proved.
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

NOTICE='ws53-km: assumed slot/clear-loader profile; no silicon ownership or erasure proof'
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def material(slot,length): return bytes((i*17+slot+length+3)&255 for i in range(length))


@contextmanager
def running(args,name,address):
    folder=args.output/name
    folder.mkdir()
    log=[]
    with tempfile.TemporaryDirectory(prefix='ws53-km-cpu-') as temporary:
        temp=Path(temporary)
        argv=[str(args.qemu.resolve()),'-M','ws53','-S','-display','none','-monitor','none',
              '-serial','null','-kernel',str(args.elf.resolve()),'-global','ws53-km.assume-profile=on',
              '-icount','shift=0,sleep=on','-qtest',f'unix:{temp}/qt,server=on,wait=off',
              '-qtest-log','/dev/null','-qmp',f'unix:{temp}/qm,server=on,wait=off','-d','guest_errors']
        proc=subprocess.Popen(argv,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        try:
            with connect(temp/'qt',proc) as qs,qs.makefile('rwb',buffering=0) as qt,connect(temp/'qm',proc) as ms,ms.makefile('rwb',buffering=0) as qm:
                json.loads(qm.readline())
                def qmp(name,arguments=None):
                    request=dict(execute=name)
                    if arguments is not None: request['arguments']=arguments
                    qm.write(json.dumps(request).encode()+b'\n')
                    while True:
                        value=json.loads(qm.readline()); assert 'error' not in value,value
                        if 'return' in value:
                            log.append(dict(qmp=request,result=value['return']))
                            return value['return']
                def command(text):
                    parts=text.split()
                    if parts[0].startswith(('write','out')):
                        assert parts[0]=='writel' and int(parts[1],0) in {address+4*i for i in (0,5,6,7,8,9,10)},text
                    qt.write(text.encode()+b'\n'); value=qt.readline().decode().split()
                    assert value and value[0]=='OK',(text,value)
                    log.append(dict(qtest=text,result=value[1:]))
                    return value[1:]
                def report():
                    raw=bytes.fromhex(command(f'read {address:#x} 96')[0].removeprefix('0x'))
                    value=struct.unpack('<24I',raw)
                    assert value[12]==0,('Rust panic',value)
                    return value
                def wait(predicate):
                    end=time.monotonic()+5
                    while True:
                        value=report()
                        if predicate(value): return value
                        assert time.monotonic()<end,('CPU progress timeout',value)
                        time.sleep(.001)
                def invoke(op,**params):
                    for key,value in params.items():
                        index=dict(slot=5,algorithm=6,length=7,parity=8,usage=9,budget=10)[key]
                        command(f'writel {address+4*index:#x} {value}')
                    seq=report()[1]; command(f'writel {address:#x} {op}'); qmp('cont')
                    wait(lambda v:v[1]==seq+1); qmp('stop')
                    value=report(); assert value[0]==0 and value[1]==seq+1
                    return value
                def get(name): return qmp('qom-get',dict(path='/machine/km',property=name))
                def inject(name,value): qmp('qom-set',dict(path='/machine/km',property=name,value=value))
                def fault(number,on=True): command(f'set_irq_in /machine/km fault {number} {int(on)}')
                qmp('qmp_capabilities')
                try:
                    qmp('cont'); wait(lambda v:v[2]==1); qmp('stop')
                    yield invoke,get,inject,fault,report
                finally: qmp('quit')
            stdout,stderr=proc.communicate(timeout=5)
            assert proc.returncode==0 and not stdout
            assert stderr.decode().splitlines()==[NOTICE],stderr.decode()
        finally:
            if proc.poll() is None: proc.terminate()
            stdout,stderr=proc.communicate(timeout=5)
            (folder/'stdout.log').write_bytes(stdout); (folder/'stderr.log').write_bytes(stderr)
            (folder/'operations.json').write_text(json.dumps(log,indent=2)+'\n')
            (folder/'command.json').write_text(json.dumps(argv,indent=2)+'\n')


def run(args):
    assert not args.output.exists()
    contract=elf_check.validate(args.elf)
    with args.elf.open('rb') as stream:
        elf=ELFFile(stream)
        symbols=elf.get_section_by_name('.symtab').get_symbol_by_name('WS53_KM_MODEL')
        assert len(symbols)==1 and symbols[0]['st_size']==96
        address=symbols[0]['st_value']
        assert address%4==0 and 0x20010000<=address<=0x20060000-96
    args.output.mkdir(parents=True)
    results=[]
    cases=[]
    for slot in range(8):
        for algorithm,length in enumerate((16,24,32,16)):
            for usage in (1,2,3):
                for parity in (0,1):
                    cases.append((f'cipher-{slot}-{algorithm}-{usage}-{parity}',slot,algorithm,length,usage,parity,
                                  'parity' if algorithm in (1,2) and parity else 'normal',0))
    for slot in (8,9):
        for algorithm in range(4,10):
            maximum=128 if algorithm in (7,8) else 64
            for length in (0,1,15,16,63,64,maximum):
                # Duplicate maximum64 is intentionally deduplicated below.
                cases.append((f'hmac-{slot}-{algorithm}-{length}',slot,algorithm,length,1,0,'normal',0))
            if maximum==64: cases.append((f'hmac-{slot}-{algorithm}-too-long',slot,algorithm,65,1,0,'length',0))
    cases=list(dict.fromkeys(cases))
    for slot,algorithm,length in ((0,2,32),(7,2,32),(8,8,128),(9,8,128)):
        for case in ('drop','forget','reject-claim','pre-error','foreign-irq','ack-stuck',
                     'flush-stall','flush-timeout','flush-fail','keep-owner'):
            cases.append((f'{case}-{slot}',slot,algorithm,length,3,0,case,0))
        for case in ('load-stall','lost-owner'):
            for stage in range(2 if slot<8 else 8):
                cases.append((f'{case}-{slot}-{stage}',slot,algorithm,length,3,0,case,stage))
    for name,slot,algorithm,length,usage,parity,case,stage in cases:
        with running(args,name,address) as (invoke,get,inject,fault,report):
            maximum=128 if algorithm in (7,8) else 64 if algorithm>=4 else ((length+15)//16)*16
            if case=='reject-claim': fault(1)
            if case=='pre-error': inject('inject-controller-error',0x80000000)
            if case=='foreign-irq': inject('inject-raw',1)
            if case=='ack-stuck': fault(3)
            if case in ('load-stall','lost-owner'):
                inject('fault-after-submissions',stage); fault(0 if case=='load-stall' else 2)
            before=(get('reads'),get('writes'))
            value=invoke(1,slot=slot,algorithm=algorithm,length=length,usage=usage,parity=parity,budget=32)
            expected_error=dict(parity=1,length=100,**{'reject-claim':5,'pre-error':6,'foreign-irq':10,'ack-stuck':11,'load-stall':12,'lost-owner':5}).get(case,0)
            assert value[4]==expected_error,(name,value)
            keys=[bytes.fromhex(x) for x in get('debug-keys').split(':')]
            if case in ('parity','length'):
                assert (get('reads'),get('writes'))==before
                assert value[13:16]==(0,0,0) and keys==[bytes(128)]*10
            elif expected_error:
                assert value[15]==expected_error
                assert value[13]==(0 if case=='pre-error' else 1<<slot) and value[14]==0
                if case in ('load-stall','lost-owner'):
                    assert get('completed-blocks')==stage and get('submitted-blocks')==stage+1
                    assert keys[slot][:16*stage]==material(slot,length)[:16*stage]
                    assert keys[slot][16*stage:]==bytes(128-16*stage)
                    assert bytes.fromhex(get('debug-staging'))==material(slot,length)[stage*16:stage*16+16]
                count=(get('reads'),get('writes'))
                assert invoke(1)[4]==expected_error
                assert invoke(6)[15]==expected_error
                assert (get('reads'),get('writes'))==count
            else:
                assert value[13:16]==(1<<slot,1<<slot,0)
                assert value[16:19]==(algorithm,slot,parity if algorithm<4 else 0)
                padded=material(slot,length)+bytes(maximum-length)
                start=16 if algorithm in (0,3) and parity else 0
                expected=bytearray(128); expected[start:start+len(padded)]=padded
                assert keys[slot]==expected and all(key==bytes(128) for i,key in enumerate(keys) if i!=slot)
                assert bytes.fromhex(get('debug-staging'))==bytes(16)
                assert get('completed-blocks')==maximum//16
                assert invoke(2)[4]==0
                if case in ('drop','forget'):
                    count=(get('reads'),get('writes'))
                    assert invoke(4 if case=='drop' else 5)[4]==0
                    assert invoke(1)[4]==3
                    assert (get('reads'),get('writes'))==count
                    assert report()[13:15]==(1<<slot,1<<slot)
                else:
                    release_fault={'flush-stall':4,'flush-timeout':5,'flush-fail':6,'keep-owner':7}.get(case)
                    if release_fault is not None: fault(release_fault)
                    value=invoke(3)
                    error={'flush-stall':13,'flush-timeout':7,'flush-fail':7,'keep-owner':14}.get(case,0)
                    assert value[4]==error,(name,value)
                    if error:
                        assert value[13:16]==(1<<slot,0,error)
                        count=(get('reads'),get('writes'))
                        assert invoke(1)[4]==error
                        assert (get('reads'),get('writes'))==count
                    else:
                        assert value[13:16]==(0,0,0) and get('released-slots')==1
                        assert [bytes.fromhex(x) for x in get('debug-keys').split(':')]==keys
                        assert invoke(1)[4]==0, 'released slot could not be reused'
            assert get('rejections')==0,(name,get('rejections'))
            results.append(dict(name=name,case=case,slot=slot,algorithm=algorithm,length=length,
                                stage=stage,passed=True,final=list(report())))
        print('PASS '+name,flush=True)
    # Concurrent Rust leases distinguish equal numeric cipher/HMAC slots.
    with running(args,'all-ten-slots',address) as (invoke,get,inject,fault,report):
        for slot in range(10):
            value=invoke(1,slot=slot,algorithm=0 if slot<8 else 6,length=16,usage=3,parity=0,budget=32)
            assert value[4]==0 and value[13]==(1<<(slot+1))-1
        assert report()[13:16]==(1023,1023,0)
        for slot in range(10): assert invoke(2,slot=slot)[4]==0
        for slot in range(10):
            value=invoke(3,slot=slot)
            assert value[4]==0 and value[13]==1023 & ~((1<<(slot+1))-1)
        assert get('released-slots')==10 and get('rejections')==0
        results.append(dict(name='all-ten-slots',case='concurrent',passed=True))
    summary=dict(passed=True,elf_sha256=sha(args.elf),qemu_sha256=sha(args.qemu),script_sha256=sha(Path(__file__)),
                 contract=contract,mailbox=hex(address),cases=len(results),
                 counts={case:sum(row['case']==case for row in results) for case in sorted({row['case'] for row in results})},
                 hardware_qualified=False,cipher_algorithm_tested=False,results=results)
    (args.output/'report.json').write_text(json.dumps(summary,indent=2)+'\n')
    return summary


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--qemu',type=Path,required=True)
    parser.add_argument('--elf',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args(); result=run(args)
    print(json.dumps({k:v for k,v in result.items() if k!='results'},indent=2))
