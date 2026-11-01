#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Same RAM ELF executes the measured UART1/H0 gate; strict MMIO diagnostics."""
import argparse
import hashlib
import json
from pathlib import Path
import ws53_uart_clock_build as builder
import ws53_uart_clock_probe as contract
import ws53_ci_qemu_uart_clock_build as policy
from ws53_qemu_fixture import run_fixture

ROOT=Path(__file__).resolve().parents[1]
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()

# Exact model fixture transcript, independent of hardware-driving HIL tools.
EXPECTED=(b'WS53 RAM data/bss OK\r\nWS53 PASS timer_irq=53 count=3\r\n'
    b'WS53 PASS uart2_line cases=72 loopback_bytes=144 console_restored=1\r\n'
    b'WS53 PASS uart2_flush delayed=48 rx_preserved=144 fifo_empty_gap=1\r\n'
    b'WS53 PASS uart_all_lines ports=3 cases=216 rx_preserved=432 delayed=144\r\n'
    + contract.END)


def fnv(data):
    value=14695981039346656037
    for byte in data:value=((value^byte)*1099511628211)&((1<<64)-1)
    return value


def trace(path,rows):
    values=[list(map(int,line.split(','))) for line in path.read_text().splitlines()]
    edges={p:[] for p in (17,12,8)};previous={p:0 for p in edges}
    for t,p,level,oe,pull,contention in values:
        if p in edges:
            contract.require(not contention,'clock model pad contention')
            if level!=previous[p]:edges[p].append((t,level));previous[p]=level
    pulses=[];begin=None
    for sample,level in edges[17]:
        if level:begin=sample
        elif begin is not None:
            if 900000<=sample-begin<=1500000:pulses.append((begin,sample))
            begin=None
    contract.require(len(pulses)==6,'model marker framing')
    measured=[]
    for i,((begin,end),row) in enumerate(zip(pulses,rows)):
        pad=12 if row['port']==0 else 8
        n=sum(begin+100000<=t<end-100000 for t,_ in edges[pad])
        expected_pause=row['port']==1 and row['phase']==1
        contract.require((n==0) if expected_pause else n>=60,'model UART gate binding')
        timer=row['marker_ticks']*1_000_000_000//32000000
        contract.require(abs((end-begin)-timer)<=max(30000,timer//20),'model TIMER/marker mismatch')
        measured.append(dict(port=row['port'],phase=row['phase'],central_edges=n,paused=n==0,marker_ns=end-begin))
    return measured


def run(a):
    builder.validate(a.receipt,a.elf);info=contract.inspect(a.elf)
    admitted=policy.validate_receipt(a.model_receipt,a.qemu,source_root=a.model_source)
    a.output.mkdir(parents=True,exist_ok=False);pad=a.output.resolve()/'pads.csv'
    expected=EXPECTED
    streams={12:b'\x55'*48,8:b'\x55'*48,17:b'',33:expected}
    qom=[('/machine/pinctrl',f'uart-tx-{kind}-{p}',len(v) if kind=='bytes' else fnv(v))
        for p,v in streams.items() for kind in ('bytes','hash')]
    notice='ws53-uart: assumed byte DMA requests, FIFO64 and timed serializer'
    wire='ws53-uart: TX pad8N1 profile assumes stable32MHz; RX pads unmodeled'
    gpio=[f'qemu-system-riscv32: ws53-gpio: opaque SW_CTL observation enabled (width={w}); selector encoding and silicon reset are not modeled' for w in (23,12,3)]
    result=run_fixture(str(a.qemu),a.elf,device='uart-pads',begin='',passed='',done=contract.END.decode().rstrip('\n'),
        expected_output=expected,expected_diagnostics=[notice,wire]*3+gpio,qom_checks=qom,
        qemu_options=('-global','ws53-uart.assume-dma-profile=on','-global','ws53-uart.char-ns=100000',
            '-global','ws53-uart.div-l-read-requires-enable=on','-global','ws53-uart.assume-pad-tx=on',
            '-global','ws53-pinctrl.assume-uart-tx=on','-global','ws53-pinctrl.uart-console-profile=on',
            '-global','ws53-gpio.assume-source-control=on','-chardev',f'file,id=clocktrace,path={pad}',
            '-global','ws53-pinctrl.pad-trace=clocktrace'),
        ram_regions=((info['address'],56),),register_checks=((0x52000044,0x11),),
        register_mask_checks=((0x52002210,1,0),))
    words=result['ram_snapshots'][0]['words'];observation=contract.evaluate(words)
    waves=trace(pad,observation['rows'])
    report=dict(passed=True,elf_sha256=sha(a.elf),source_bound_model=admitted,model_report=result,
        words=words,observation=observation,waves=waves,hardware_accessed=False,flash_qualification=False,
        source_clock_frequency_or_cold_boot_qualified=False,
        inputs_sha256={str(p.resolve()):sha(p) for p in (Path(__file__),Path(contract.__file__),Path(builder.__file__),
            Path(policy.__file__),ROOT/'scripts/ws53_qemu_fixture.py',a.elf,a.receipt,a.model_receipt,pad)})
    (a.output/'report.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(dict(passed=True,model_rows=len(waves))))


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('qemu','model-receipt','model-source','elf','receipt','output'):p.add_argument('--'+name,type=Path,required=True)
    args=p.parse_args()
    for name,value in vars(args).items():setattr(args,name,value.resolve())
    run(args)
