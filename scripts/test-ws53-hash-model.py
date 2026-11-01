#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Run the production WS53 HASH HAL on a RISC-V CPU with an opt-in QEMU profile.

Only command/parameter mailbox words are written by the host. The CPU prepares
messages/descriptors and performs all MMIO. Fault inputs describe a virtual peer;
no hardware ownership, coherence, DMA drain or Flash qualification is implied.
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

NOTICE = 'ws53-hash: assumed two-channel REE/ACK/ring/DMA profile; no ownership or physical drain proof'
ALGORITHMS = ('sha1','sha224','sha256','sha384','sha512','sm3')


def digest(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def message(length, kind=0):
    if kind == 1:
        assert length == 3
        return b'abc'
    if kind == 2: return (b'abcd'*((length+3)//4))[:length]
    return bytes((i*37+11)&255 for i in range(length))
def padded(data, algorithm):
    block, suffix = (128,16) if algorithm in ('sha384','sha512') else (64,8)
    size = ((len(data)+1+suffix+block-1)//block)*block
    return data+b'\x80'+bytes(size-len(data)-1-8)+(len(data)*8).to_bytes(8,'big')


@contextmanager
def running(args, name, address, *, words=40, expected_diagnostics=()):
    log=[]
    folder=args.output/name
    folder.mkdir()
    with tempfile.TemporaryDirectory(prefix='ws53-hash-hal-') as temp:
        temp=Path(temp)
        argv=[str(args.qemu.resolve()),'-M','ws53','-S','-display','none','-monitor','none','-serial','null',
              '-kernel',str(args.elf.resolve()),'-global','ws53-hash.assume-profile=on',
              '-icount','shift=0,sleep=on',
              '-qtest',f'unix:{temp}/qt,server=on,wait=off','-qtest-log','/dev/null',
              '-qmp',f'unix:{temp}/qm,server=on,wait=off','-d','guest_errors',
              ]
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
                    if text.split()[0].startswith(('write', 'out')):
                        parts=text.split()
                        assert parts[0]=='writel' and int(parts[1],0) in {address+4*i for i in (0,5,6,7,34)}, text
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
            assert stderr.decode().splitlines()==[NOTICE,*expected_diagnostics],stderr.decode()
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
    contract = elf_check.validate(args.elf)
    addresses = {}
    with args.elf.open('rb') as source:
        elf = ELFFile(source)
        for suffix, size, alignment in (('MODEL',160,4), ('DATA',320,32), ('TABLE',32,32)):
            symbols = elf.get_section_by_name('.symtab').get_symbol_by_name('WS53_HASH_'+suffix)
            assert len(symbols) == 1 and symbols[0]['st_size'] == size
            address = symbols[0]['st_value']
            assert address % alignment == 0 and 0x20010000 <= address <= 0x20060000-size
            addresses[suffix] = address
    regions = sorted((addresses[n], addresses[n]+s) for n,s in
                     (('MODEL',160), ('DATA',320), ('TABLE',32)))
    assert all(a[1] <= b[0] for a,b in zip(regions,regions[1:]))
    cases = []
    for channel in (1,2):
        for algorithm in ALGORITHMS:
            maximum = 239 if algorithm in ('sha384','sha512') else 247
            for length in (0,3,55,56,63,64,111,112,127,128,maximum):
                cases.append((f'{algorithm}-ch{channel}-len{length}',algorithm,channel,length,0,'digest'))
            for length,kind in ((3,1),(64,2)):
                cases.append((f'{algorithm}-ch{channel}-vector{kind}',algorithm,channel,length,kind,'digest'))
            cases.append((f'{algorithm}-ch{channel}-too-long',algorithm,channel,maximum+1,0,'format'))
        for fault in ('reject-claim','clear-stall','clear-ack','pre-error','dma-stall',
                      'suppress-done','lost-owner','post-error','done-ack','cancel-accepted',
                      'drop-accepted','forget-accepted','bus-descriptor','bus-accepted','cancel-new'):
            cases.append((f'{fault}-ch{channel}','sha256',channel,64,0,fault))
    args.output.mkdir(parents=True)
    results = []
    for name,algorithm,channel,length,kind,case in cases:
        expected_diagnostics = ()
        if case in ('bus-descriptor','bus-accepted'):
            reason = 'descriptor-read-error' if case == 'bus-descriptor' else 'DMA-read-error'
            expected_diagnostics = (f'ws53-hash: unimplemented/unavailable {reason} +{0x5000+channel*256:#x}',)
        with running(args,name,addresses['MODEL'],expected_diagnostics=expected_diagnostics) as (cmd,qmp,invoke,report):
            def read(address, size):
                return bytes.fromhex(cmd(f'read {address:#x} {size}')[0].removeprefix('0x'))
            def get(name): return qmp('qom-get',dict(path='/machine/hash',property=name))
            def inject(name, value):
                qmp('qom-set',dict(path='/machine/hash',property='inject-'+name,value=value))
            def fault(number, enabled=True):
                cmd(f'set_irq_in /machine/hash fault {(channel-1)*8+number} {int(enabled)}')
            def run_until(predicate):
                qmp('cont')
                deadline = time.monotonic()+5
                try:
                    while not predicate():
                        assert time.monotonic() < deadline, 'model progress timeout'
                        time.sleep(.001)
                finally:
                    qmp('stop')
            for word,value in ((5,ALGORITHMS.index(algorithm)),(6,channel),(7,length),(34,kind)):
                cmd(f'writel {addresses["MODEL"]+word*4:#x} {value}')
            data = message(length,kind)
            encoded = padded(data,algorithm)
            value = invoke(1)
            assert value[14:16] == (addresses['DATA']+32,addresses['TABLE'])
            assert get('descriptors') == get('bytes-read') == get('transfers') == 0
            if case == 'format':
                assert value[4] == 23 and value[13] == 0, value
                assert read(addresses['DATA'],320) == b'\xa5'*320
                assert read(addresses['TABLE'],32) == bytes(32)
            else:
                assert value[3:5] == (1,0) and value[13] == len(encoded), value
                expected_data = b'\xa5'*32+encoded+b'\xa5'*(256-len(encoded))+b'\xa5'*32
                expected_table = struct.pack('<IIII',3,len(encoded),addresses['DATA']+32,0)+bytes(16)
                assert read(addresses['DATA'],320) == expected_data
                assert read(addresses['TABLE'],32) == expected_table
                # NotStarted must be retryable; host never initializes MMIO.
                assert invoke(3)[4] == 1
                if case == 'cancel-new':
                    assert invoke(5)[3:5] == (5,0)
                    assert invoke(3)[4] == 10
                    assert invoke(2)[4] == 2
                    assert get('descriptors') == 0
                else:
                    start_fault = {'reject-claim':4,'clear-stall':0,'clear-ack':6,
                        'dma-stall':1,'suppress-done':3,'lost-owner':1,'post-error':1,
                        'done-ack':1,'cancel-accepted':7,'drop-accepted':7,'forget-accepted':7,
                        'bus-descriptor':2,'bus-accepted':7}.get(case)
                    if start_fault is not None: fault(start_fault)
                    if case == 'pre-error':
                        inject('tee-cause',0x80000000); inject('tee-status',0xa55a0000|channel<<16)
                        inject('ree-cause',0x55); inject('ree-status',0x5aa50000|channel<<16)
                    value = invoke(2)
                    start_error = {'reject-claim':3,'clear-stall':5,'clear-ack':6,'pre-error':8}.get(case)
                    if start_error is not None:
                        assert value[4] == start_error and get('descriptors') == 0, value
                        if case == 'clear-stall': assert value[11] == 254
                        if case == 'pre-error':
                            assert value[9:11] == (0x80000000,0x55)
                            assert value[32:34] == (0xa55a0000|channel<<16,0x5aa50000|channel<<16)
                        if start_fault is not None: fault(start_fault,False)
                        inject('tee-cause',0); inject('ree-cause',0)
                        assert invoke(3)[4] == start_error
                        assert invoke(4)[4] == start_error
                    else:
                        assert value[3:5] == (2,0), value
                        assert invoke(2)[4] == 2
                        if case == 'digest':
                            value = invoke(4)
                            assert value[3:5] == (4,0), value
                            expected = hashlib.new(algorithm,data).digest()
                            observed = b''.join(v.to_bytes(4,'little') for v in value[16:32])
                            assert value[8] == len(expected) and observed[:len(expected)] == expected
                            assert observed[len(expected):] == bytes(64-len(expected))
                            # Completion returns a copied, cached digest even if
                            # later hardware flags change; no new DMA is started.
                            inject('ree-cause',0xff)
                            assert invoke(3)[8:11] == (len(expected),0,0)
                            assert invoke(4)[3:5] == (4,0)
                            assert get('descriptors') == get('transfers') == 1
                            assert get('bytes-read') == len(encoded)
                        elif case in ('lost-owner','post-error'):
                            if case == 'lost-owner': fault(5)
                            else:
                                inject('ree-cause',0x80000000); inject('ree-status',channel<<16)
                            error = 4 if case == 'lost-owner' else 8
                            value = invoke(3)
                            assert value[4] == error, value
                            if case == 'post-error':
                                assert value[10] == 0x80000000 and value[33] == channel<<16
                            fault(1,False)
                            run_until(lambda:get('transfers')==1)
                            assert invoke(4)[4] == error
                        elif case == 'done-ack':
                            fault(6); fault(1,False)
                            run_until(lambda:get('transfers')==1)
                            assert invoke(3)[4] == 6
                            fault(6,False)
                            assert invoke(4)[4] == 6
                        elif case in ('cancel-accepted','drop-accepted','forget-accepted','bus-accepted'):
                            run_until(lambda:get('pending-reads')==1)
                            assert get('bytes-read') == 0
                            if case == 'bus-accepted': fault(2)
                            else:
                                op = {'cancel-accepted':5,'drop-accepted':6,'forget-accepted':7}[case]
                                assert invoke(op)[3:5] == (op,0)
                                assert invoke(3)[4] == (10 if op==5 else 102)
                            fault(7,False)
                            if case == 'bus-accepted':
                                run_until(lambda:get('rejections')==1)
                                assert invoke(4)[4] == 9 and get('bytes-read') == 0
                            else:
                                run_until(lambda:get('transfers')==1)
                                assert get('bytes-read') == len(encoded)
                                assert invoke(3)[4] == (10 if case=='cancel-accepted' else 102)
                        elif case in ('dma-stall','suppress-done','bus-descriptor'):
                            before = report()[11]
                            value = invoke(4)
                            assert value[4] == 9 and value[11]-before == 254, value
                            if case == 'dma-stall':
                                assert get('bytes-read') == get('transfers') == 0
                                fault(1,False)
                                run_until(lambda:get('transfers')==1)
                            elif case == 'suppress-done':
                                assert get('transfers') == 1 and get('bytes-read') == len(encoded)
                            else: assert get('rejections') == 1 and get('bytes-read') == 0
                            assert invoke(3)[4] == 9
                        else: raise AssertionError(case)
                assert read(addresses['DATA'],320) == expected_data
                assert read(addresses['TABLE'],32) == expected_table
            assert get('rejections') == int(case in ('bus-descriptor','bus-accepted'))
            results.append(dict(case=name,passed=True,final_mailbox=list(report()),
                descriptors=get('descriptors'),bytes_read=get('bytes-read'),transfers=get('transfers'),
                rejections=get('rejections'),allocations_retained=True))
    result = dict(passed=True,cases=results,addresses=addresses,elf_contract=contract,
        digest_cases=sum(c[-1]=='digest' for c in cases),format_rejections=12,fault_cases=30,
        elf_sha256=digest(args.elf),qemu_sha256=digest(args.qemu),script_sha256=digest(Path(__file__)),
        fixture_helper_sha256=digest(Path(__file__).with_name('ws53_qemu_fixture.py')),
        host_paced_fixture=True,host_mmio_setup=False,hardware_accessed=False,
        physical_drain_verified=False,cache_coherency_verified=False,interrupt_handler_tested=False,
        flash_boot_tested=False,model_delay_is_calibrated=False)
    (args.output/'report.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for option in ('qemu','elf','output'): parser.add_argument('--'+option,type=Path,required=True)
    run(parser.parse_args())
