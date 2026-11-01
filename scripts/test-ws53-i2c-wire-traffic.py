#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Real Rust/PAC normal transfers with independently decoded virtual wires."""
import argparse
import hashlib
import json
from pathlib import Path
import ws53_i2c_traffic_build as builder
import ws53_ci_qemu_uart_clock_build as policy
from ws53_i2c_wire_trace import transactions
from ws53_qemu_fixture import run_fixture

sha = lambda p:hashlib.sha256(p.read_bytes()).hexdigest()


def expected_exercise():
    rows = []
    def add(data,acks,ending='stop'):
        rows.append(dict(bytes=data,acks=acks,ending=ending))
    for ten in (False,True):
        write = [0xf4,0x50] if ten else [0xa0]
        read = [0xf5] if ten else [0xa1]
        add(write+[0x80,0x12,0x34,0x56],[True]*(len(write)+4))
        add(write+[0x80],[True]*(len(write)+1),'restart')
        add(read+[0x12,0x34,0x56],[True]*3+[False])
        add(write+[0],[True]*(len(write)+1),'restart')
        add(read+[i^0xa5 for i in range(100)],[True]*100+[False])
        if ten:
            add(write,[True]*2,'restart')
        add(read+[100^0xa5],[True,False],'restart')
        add(write+[0x80,0x12],[True]*(len(write)+2))
    add([0xa2],[False])
    add([0xa0,0],[True,True])
    add([0xf4,0x51],[True,False])
    add([0xa0,0],[True,True])
    return rows


def run(a):
    builder.validate(a.receipt,a.elf)
    admitted = policy.validate_receipt(a.model_receipt,a.qemu,source_root=a.model_source)
    a.output.mkdir(parents=True,exist_ok=False)
    pad = a.output.resolve()/'pads.csv'
    notice = 'ws53-i2c: assumed32MHz master wire/virtual-peer profile; pull/filter delay unmodeled'
    result = run_fixture(str(a.qemu),a.elf,device='i2c',begin='WS53 I2C QEMU begin',
        done='WS53 I2C QEMU done',
        passed='WS53 PASS I2C instances=2 addr7=1 addr10=1 repeated_start=1 long_read=100 nack_recovery=1',
        qemu_options=('-global','ws53-i2c.assume-profile=on','-global','ws53-i2c.assume-wire-profile=on',
            '-global','ws53-pinctrl.assume-i2c-wire=on','-chardev',f'file,id=trace,path={pad}',
            '-global','ws53-pinctrl.pad-trace=trace'),
        expected_diagnostics=[notice,'ws53-i2c: assumed profile 32MHz FIFO8 CON-addressing deferred-read-ACK disable=1us'])
    decoded = []
    # The leading configuration trace is outside all STARTs, so skip through
    # the final pad setup event. It must precede the first I2C edge on either bus.
    values = [tuple(map(int,line.split(','))) for line in pad.read_text().splitlines()]
    first_drive = min(time for time,pin,_,drive,_,_ in values if pin in (47,22,8,9) and drive)
    begin = first_drive-1
    expected = expected_exercise()
    for bus,scl,sda,repeat in ((0,47,22,2),(1,8,9,1)):
        wire = transactions(pad,scl=scl,sda=sda,begin=begin)
        if wire['segments'] != expected*repeat:
            (a.output/f'bus{bus}-difference.json').write_text(json.dumps(dict(
                actual=wire,expected=expected*repeat),indent=2)+'\n')
            raise ValueError(f'I2C{bus} normal virtual-peer wire bytes/ACKs differ')
        decoded.append(dict(bus=bus,segments=wire['segments'],exercises=repeat))
    # Recheck both source-bound builds after execution as well: a successful
    # trace must not be attributed to a different binary or source closure.
    builder.validate(a.receipt,a.elf)
    if policy.validate_receipt(a.model_receipt,a.qemu,source_root=a.model_source) != admitted:
        raise ValueError('I2C model admission changed during the run')
    report = dict(passed=True,elf_sha256=sha(a.elf),model_admission=admitted,result=result,wires=decoded,
        pad_trace_sha256=sha(pad),hardware_accessed=False,physical_peer_or_flash_qualified=False,
        inputs_sha256={str(p.resolve()):sha(p) for p in (Path(__file__),Path(builder.__file__),
            Path(policy.__file__),Path(__file__).with_name('ws53_i2c_wire_trace.py'),a.elf,a.receipt,a.model_receipt)})
    (a.output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(dict(passed=True,buses=2,exercises=3,segments=sum(len(w['segments']) for w in decoded))))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    for name in ('qemu','model-receipt','model-source','elf','receipt','output'):
        p.add_argument('--'+name,type=Path,required=True)
    run(p.parse_args())
