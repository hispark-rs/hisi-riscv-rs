#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Actual Rust UART0/1 DMA ownership and serializer tests; no physical hardware."""
import argparse
import json
import os
from pathlib import Path
import selectors
import subprocess
import tempfile
import time
from elftools.elf.elffile import ELFFile
from ws53_qemu_fixture import connect, elf_check, require

BASE_NOTICE = 'ws53-mdma: assumed DTCM mem-to-mem same-width posted-write profile'
UART_NOTICES = ['ws53-mdma: assumed UART0/1 byte handshakes with flow1/2, master0/prot0',
                *['ws53-uart: assumed byte DMA requests, FIFO64 and timed serializer']*2]


def run(qemu, elf, address, port, ch, receive, scenario, *, async_mode=False):
    length = 96 if scenario == 'Q' else 17
    pattern = bytes((port*53+ch*29+i*13+7)&255 for i in range(length))
    uart_base = 0x52081000 if port == 0 else 0x52080000
    request = (10 if port == 0 else 8)+int(receive)
    with tempfile.TemporaryDirectory(prefix='ws53-uart-dma-hal-') as temp:
        work = Path(temp); qtpath=work/'qtest'; qmpath=work/'qmp'
        uarts=[work/'uart0',work/'uart1']
        args=[qemu,'-M','ws53','-S','-display','none','-monitor','none',
              '-kernel',str(elf),'-icount','shift=0,sleep=on','-global','ws53-mdma.assume-profile=on',
              '-qmp',f'unix:{qmpath},server=on,wait=off','-qtest',f'unix:{qtpath},server=on,wait=off',
              '-qtest-log','/dev/null','-d','guest_errors']
        args+=['-global','ws53-mdma.assume-uart=on']
        for i,path in enumerate(uarts):
            args+=['-chardev',f'socket,id=u{i},path={path},server=on,wait=off','-serial',f'chardev:u{i}']
        args+=['-serial','stdio']
        proc=subprocess.Popen(args,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        output=bytearray()
        try:
            with connect(qtpath,proc) as qs, qs.makefile('rwb',buffering=0) as qt, \
                 connect(qmpath,proc) as ms, ms.makefile('rwb',buffering=0) as qm, \
                 connect(uarts[port],proc) as peer, selectors.DefaultSelector() as selector:
                json.loads(qm.readline()); selector.register(proc.stdout,selectors.EVENT_READ)
                def execute(name,args=None):
                    q={'execute':name}
                    if args is not None:q['arguments']=args
                    qm.write(json.dumps(q).encode()+b'\n')
                    while True:
                        response=json.loads(qm.readline());require('error' not in response,str(response))
                        if 'return' in response:return response['return']
                def cmd(text):
                    qt.write(text.encode()+b'\n'); r=qt.readline().split();require(r and r[0]==b'OK',str(r));return r[1:]
                def read(a):return int(cmd(f'readl {a:#x}')[0],0)
                def write(a,v):cmd(f'writel {a:#x} {v:#x}')
                def prop(path,name):return execute('qom-get',{'path':path,'property':name})
                def dma(name):return prop('/machine/mdma',name)
                def fault(name,on=True):cmd(f'set_irq_in /machine/mdma {name} {ch} {int(on)}')
                def txstall(on):cmd(f'set_irq_in /machine/uart{port} tx-stall 0 {int(on)}')
                def send(data):proc.stdin.write(data);proc.stdin.flush()
                def until(line):
                    deadline=time.monotonic()+10
                    while (line+'\n').encode() not in output:
                        require(time.monotonic()<deadline,f'{port}/{ch}/{receive}/{scenario} timeout {output!r}')
                        for key,_ in selector.select(.02):
                            chunk=os.read(key.fd,4096);require(bool(chunk),'guest exit');output.extend(chunk)
                        if b'WS53 UARTDMA PANIC' in output:
                            execute('stop')
                            snapshot={hex(off):read(uart_base+off) for off in (0x18,0x1c,0x34,0x48,0x4c)}
                            cpu=execute('human-monitor-command',{'command-line':'info registers'})
                            raise ValueError(f'{port}/{ch}/{receive}/{scenario}: {output!r}; UART={snapshot}; transmitted={prop(f"/machine/uart{port}","transmitted")}; CPU={cpu}')
                def memory():return bytes.fromhex(cmd(f'read {address:#x} 0x62')[0].decode().removeprefix('0x'))
                def drained(expected):
                    deadline=time.monotonic()+5
                    while dma('writes')!=expected or dma('pending-writes') or read(0x52070010):
                        require(time.monotonic()<deadline,'model DMA did not drain');time.sleep(.002)
                execute('qmp_capabilities')
                try:
                    require(read(uart_base+0x60)==0xd04,'measured UART parameter changed')
                    if scenario=='C':
                        cmd(f'set_irq_in /machine/uart{port} no-dma-capability 0 1')
                        require(read(uart_base+0x60)==0x504,'missing-DMA capability fault not applied')
                    if not receive:txstall(True)
                    if receive or scenario=='S':fault('stall')
                    if scenario in 'TDFI':fault('bus-stall')
                    if scenario=='R':fault('read-error')
                    if scenario=='W':fault('write-error')
                    if scenario in 'BL':
                        write(uart_base+0x28,1);write(uart_base+0x40,0x141 if scenario=='L' else 65)
                    execute('cont');until('WS53 UARTDMA ready')
                    send(bytes([48+port,48+ch])+ (b'r' if receive else b't')+scenario.encode())
                    if scenario not in 'CBL':
                        until('WS53 UARTDMA armed')
                        cfg=read(0x52070108+ch*32)
                        require((cfg>>9)&7==(2 if receive else 1),'wrong flow')
                        require((cfg>>(1 if receive else 5))&15==request,'wrong route')
                        if receive:
                            peer.sendall(pattern);deadline=time.monotonic()+3
                            while read(uart_base+0x4c)!=length:
                                require(time.monotonic()<deadline,'UART peer data not queued');time.sleep(.002)
                            if scenario!='S':fault('stall',False)
                        send(b'G')
                        if not receive and scenario in 'NVE':
                            until('WS53 UARTDMA accepted')
                            require(dma('writes')==length and dma('pending-writes')==0,'DMA not accepted')
                            require(prop(f'/machine/uart{port}','transmitted')==0,'unexpected wire progress')
                            require(read(uart_base+0x48)==length and not read(uart_base+0x34)&0x80,'FIFO accepted implies wire idle')
                            if scenario=='E':
                                write(uart_base+0x28,1);write(uart_base+0x40,0x141)
                            if scenario in 'NE':txstall(False)
                            send(b'H')
                    until('WS53 UARTDMA retained');execute('stop')
                    pending=dma('pending-writes');before=dma('writes')
                    expected_before=length if scenario in 'NVE' else 64 if scenario=='Q' else 0
                    require(before==expected_before and pending==int(scenario in 'TDFI'),'checkpoint progress mismatch')
                    require(read(0x52070010)==(1<<ch if scenario=='F' else 0),'enabled owner mismatch')
                    require(read(0x52070004)==0,'polling transfer leaked IRQ')
                    final_count=length if scenario in 'NVEF' else 1 if scenario in 'TDI' else 64 if scenario=='Q' else 0
                    expected=bytearray([0x5a]*98)
                    if not receive:expected[1:1+length]=pattern
                    elif scenario=='N':expected[1:1+length]=pattern
                    require(memory()==bytes(expected),'memory changed before drain or guard damage')
                    if pending:
                        fault('bus-stall',False);execute('cont');drained(final_count);execute('stop')
                    require(dma('writes')==final_count and dma('pending-writes')==0 and read(0x52070010)==0,'independent drain failed')
                    require(dma('errors')==int(scenario in 'RW'),'wrong bus error count')
                    raw=(1<<(ch+8)) if scenario in 'RW' else (1<<ch) if scenario in 'NVEF' else 0
                    masked=(0x101<<ch) if async_mode and scenario=='F' else 0
                    require(read(0x5207000c)==raw and read(0x52070004)==masked,'raw/IRQ isolation')
                    if scenario not in 'CBL':
                        source=uart_base+4 if receive else address+1+final_count
                        destination=address+1+final_count if receive else uart_base+4
                        require(read(0x52070110+ch*32)==source and read(0x52070104+ch*32)==destination,'fixed DATA/memory address progress')
                        require(read(0x52070114+ch*32)&0xfff==length-final_count,'remaining count')
                    if receive:expected[1:1+final_count]=pattern[:final_count]
                    require(memory()==bytes(expected),'final memory/guard mismatch')
                    if not receive and final_count:
                        txstall(False);execute('cont');deadline=time.monotonic()+5
                        while prop(f'/machine/uart{port}','transmitted')!=final_count:
                            require(time.monotonic()<deadline,'serializer did not drain');time.sleep(.002)
                        execute('stop');wire=bytearray()
                        while len(wire)<final_count:wire.extend(peer.recv(final_count-len(wire)))
                        require(bytes(wire)==pattern[:final_count],'serialized bytes differ')
                        require(read(uart_base+0x48)==0 and read(uart_base+0x34)&0x80,'wire not idle')
                    # Restore FIFO/error history before any future UART owner.
                    for _ in range(read(uart_base+0x4c)):read(uart_base+4)
                    read(uart_base+0x34)
                    send(b'R');execute('cont');until('WS53 UARTDMA done')
                finally:
                    if proc.poll() is None:execute('quit')
            remaining,errors=proc.communicate(timeout=5);output.extend(remaining)
            require(proc.returncode==0,f'QEMU exit{proc.returncode}')
            require(errors.decode().splitlines()==[BASE_NOTICE]+UART_NOTICES,errors.decode())
            expected_lines=['WS53 UARTDMA ready']
            if scenario not in 'CBL':expected_lines+=['WS53 UARTDMA armed']
            if not receive and scenario in 'NVE':expected_lines+=['WS53 UARTDMA accepted']
            expected_lines+=['WS53 UARTDMA retained','WS53 PASS UARTDMA '+('ASYNC ' if async_mode else '')+'retained=1 data=1','WS53 UARTDMA done']
            require(output.decode().splitlines()==expected_lines,str(output))
            return {'uart':port,'channel':ch,'receive':receive,'scenario':scenario,'committed_elements':final_count,
                    'posted_at_checkpoint':pending,'memory_and_wire_verified_in_model':True,
                    'abandoned':scenario in 'DF','async':async_mode,'hardware_verified':False}
        finally:
            if proc.poll() is None:proc.terminate();proc.communicate(timeout=5)


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--qemu',required=True);p.add_argument('--elf',type=Path,required=True)
    p.add_argument('--async-smoke',action='store_true')
    a=p.parse_args();contract=elf_check.validate(a.elf)
    require((b'WS53 PASS UARTDMA ASYNC' in a.elf.read_bytes())==a.async_smoke,'async fixture/mode mismatch')
    with a.elf.open('rb') as stream:
        symbols=ELFFile(stream).get_section_by_name('.symtab').get_symbol_by_name('WS53_UART_DMA_BUFFER')
        require(symbols and len(symbols)==1 and symbols[0]['st_size']==98,'wrong UART DMA allocation')
        address=symbols[0]['st_value']
    cases=[(port,ch,rx,'N') for port in range(2) for ch in range(8) for rx in (False,True)]
    cases += [(port,7 if rx else 0,rx,s) for port in range(2) for rx in (False,True) for s in 'TRWSD FICBL'.replace(' ','')]
    cases += [(port,3,False,s) for port in range(2) for s in 'VQ']
    if a.async_smoke: cases += [(port,3,False,'E') for port in range(2)]
    results=[run(a.qemu,a.elf,address,*case,async_mode=a.async_smoke) for case in cases]
    print(json.dumps({'elf':contract,'scenarios':results,'hardware_accessed':False,'physical_drain_or_integrity_proven':False},indent=2))


if __name__=='__main__':main()
