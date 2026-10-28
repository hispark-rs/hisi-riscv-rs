#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Actual Rust/PAC full-duplex SPI DMA ownership tests in QEMU; no hardware."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import selectors
import subprocess
import tempfile
import time
from elftools.elf.elffile import ELFFile
from ws53_qemu_fixture import connect, elf_check, require

NOTICES=['ws53-spi: assumed profile 32MHz FIFO8 XOR-peer enable-crossing=1us',
         'ws53-mdma: assumed DTCM mem-to-mem same-width posted-write profile',
         'ws53-mdma: assumed SPI0/QSPI1 same-width requests, flow1/2, master0/prot2']
MAPS=[dict(control=4,enable=0,fifo=0x2c,select=0xc8,tx=0xd0,rx=0xdc,dma=0x18,clear=0xf8),
      dict(control=0,enable=8,fifo=0x60,select=0x10,tx=0x20,rx=0x24,dma=0x4c,clear=0x48)]


def run(qemu,elf,width,source,destination,port,ch,side,scenario):
    rxch=(ch+1)%8;target=rxch if side=='r' else ch
    length=33 if scenario=='N' else 3;base=0x5208d000 if port else 0x52087000;m=MAPS[port]
    mask=(1<<(width*8))-1
    pattern=[(0x12340007+port*53+ch*29+i*0x10213)&mask for i in range(length)]
    with tempfile.TemporaryDirectory(prefix='ws53-spi-dma-hal-') as temp:
        work=Path(temp);qtpath=work/'qtest';qmpath=work/'qmp'
        args=[qemu,'-M','ws53','-S','-display','none','-monitor','none','-kernel',str(elf),
              '-icount','shift=0,sleep=on','-global','ws53-mdma.assume-profile=on',
              '-global','ws53-mdma.assume-spi=on','-global','ws53-spi.assume-profile=on',
              '-qmp',f'unix:{qmpath},server=on,wait=off','-qtest',f'unix:{qtpath},server=on,wait=off',
              '-qtest-log','/dev/null','-d','guest_errors','-serial','null','-serial','null','-serial','stdio']
        proc=subprocess.Popen(args,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE);output=bytearray()
        try:
            with connect(qtpath,proc) as qs,qs.makefile('rwb',buffering=0) as qt, \
                 connect(qmpath,proc) as ms,ms.makefile('rwb',buffering=0) as qm,selectors.DefaultSelector() as selector:
                json.loads(qm.readline());selector.register(proc.stdout,selectors.EVENT_READ)
                def execute(name,args=None):
                    req={'execute':name}
                    if args is not None:req['arguments']=args
                    qm.write(json.dumps(req).encode()+b'\n')
                    while True:
                        r=json.loads(qm.readline());require('error' not in r,str(r))
                        if 'return' in r:return r['return']
                def cmd(s):
                    qt.write(s.encode()+b'\n');r=qt.readline().split();require(r and r[0]==b'OK',str(r));return r[1:]
                def read(addr):return int(cmd(f'readl {addr:#x}')[0],0)
                def write(addr,value):cmd(f'writel {addr:#x} {value:#x}')
                def prop(path,name):return execute('qom-get',{'path':path,'property':name})
                def dma(name):return prop('/machine/mdma',name)
                def reg(channel,off):return read(0x52070100+channel*32+off)
                def fault(name,on=True):cmd(f'set_irq_in /machine/mdma {name} {target} {int(on)}')
                def spi_fault(name,on=True):cmd(f'set_irq_in /machine/spi {name} {port} {int(on)}')
                def send(s):proc.stdin.write(s);proc.stdin.flush()
                def advance_until(condition):
                    execute('cont');deadline=time.monotonic()+5
                    while not condition():
                        require(time.monotonic()<deadline,f'{width}/{port}/{ch}/{scenario}: model setup did not settle');time.sleep(.002)
                    execute('stop')
                def until(line):
                    deadline=time.monotonic()+10
                    while (line+'\n').encode() not in output:
                        require(time.monotonic()<deadline,f'{width}/{port}/{ch}/{side}/{scenario}: timeout {output!r}')
                        for key,_ in selector.select(.02):
                            chunk=os.read(key.fd,4096);require(chunk,'guest exit');output.extend(chunk)
                def words(addr):
                    raw=bytes.fromhex(cmd(f'read {addr:#x} {35*width:#x}')[0].decode().removeprefix('0x'))
                    return [int.from_bytes(raw[i:i+width],'little') for i in range(0,len(raw),width)]
                def check_memory(copied):
                    require(words(source)==[0x5a]+pattern+[0x5a]*(34-length),'source/guard mismatch')
                    expected=[0x5a]+[(v^(0xa5a5a5a5&mask)) if i<copied else 0x5a for i,v in enumerate(pattern)]+[0x5a]*(34-length)
                    require(words(destination)==expected,'destination/guard mismatch')
                execute('qmp_capabilities')
                try:
                    execute('cont');until('WS53 SPIDMA ready');execute('stop')
                    gates=[(0x52000548,0x80),(0x52000574,1),(0x52001240,0x2a)] if port else [(0x52000044,0x800),(0x5200055c,1),(0x52000950,1)]
                    for addr,value in gates:
                        old=int(cmd(f'readw {addr:#x}')[0],0);cmd(f'writew {addr:#x} {old|value:#x}')
                    write(base+m['control'],(width*8-1)<<(16 if port else 13));write(base+0x14,4);write(base+m['select'],1)
                    write(base+m['enable'],1)
                    # CPU execution advances virtual time; clock_step is only
                    # available under the qtest accelerator, not this Rust guest.
                    raw_address=base+(0x34 if port else 0xbc);empty_bit=1 if port else 128
                    advance_until(lambda:read(raw_address)&empty_bit)
                    if scenario in 'TDFI':fault('bus-stall')
                    if scenario=='S':fault('stall')
                    if scenario=='R':fault('read-error')
                    if scenario=='W':fault('write-error')
                    if scenario=='C':
                        write(base+m['enable'],0);advance_until(lambda:not read(raw_address)&empty_bit)
                    if scenario=='B':spi_fault('stretch');write(base+m['fifo'],0x17)
                    if scenario in 'LZ':
                        spi_fault('inject-collision-status-only' if scenario=='Z' else 'collision');write(base+m['fifo'],0x17)
                        advance_until(lambda:prop('/machine/spi',f'frames{port}')==1)
                    send(bytes([48+port,48+ch])+scenario.encode());execute('cont')
                    if scenario not in 'CBLZ':
                        until('WS53 SPIDMA armed')
                        for channel,rx in ((ch,False),(rxch,True)):
                            cfg=reg(channel,8);ctrl=reg(channel,0x14);request=(4 if port else 6)+int(rx)
                            require((cfg>>9)&7==(2 if rx else 1) and (cfg>>(1 if rx else 5))&15==request,'route mismatch')
                            require((ctrl>>28)&7==2,'wrong protection')
                        send(b'G')
                    until('WS53 SPIDMA retained');execute('stop')
                    configured=scenario not in 'CBLZ'
                    pending=dma('pending-writes');require(pending==int(scenario in 'TDFI'),'unexpected pending beat')
                    require(read(base+m['dma'])==(3 if scenario=='F' else 0),'requests not disabled together')
                    if scenario!='F':require(read(0x52070010)==0,'a channel still enabled after stop')
                    before=length-(reg(rxch,0x14)&0xfff) if configured else 0
                    check_memory(before)
                    if scenario in 'TDFI':fault('bus-stall',False)
                    if scenario=='S':fault('stall',False)
                    if scenario=='R':fault('read-error',False)
                    if scenario=='W':fault('write-error',False)
                    if scenario=='B':spi_fault('stretch',False)
                    if scenario in 'LZ':spi_fault('inject-collision-status-only' if scenario=='Z' else 'collision',False)
                    execute('cont');deadline=time.monotonic()+5
                    while dma('pending-writes') or read(0x52070010) or read(base+m['tx']):
                        require(time.monotonic()<deadline,'model transaction/serializer drain timed out');time.sleep(.002)
                    execute('stop')
                    copied=length-(reg(rxch,0x14)&0xfff) if configured else 0
                    expected=length if scenario in 'NF' else 1 if side=='r' and scenario in 'TDI' else 0
                    require(copied==expected,f'RX progress {copied} != {expected}')
                    check_memory(copied)
                    if configured:
                        tx_count=length-(reg(ch,0x14)&0xfff)
                        require(reg(ch,0x10)==source+width+tx_count*width and reg(ch,4)==base+m['fifo'],'TX address progress')
                        require(reg(rxch,0x10)==base+m['fifo'] and reg(rxch,4)==destination+width+copied*width,'RX address progress')
                        raw=read(0x5207000c)
                        require((raw>>8)==((1<<target) if scenario in 'RW' else 0),'raw errors mismatch')
                        require((raw&255)==((int(tx_count==length)<<ch)|(int(copied==length)<<rxch)),'raw TC mismatch')
                    # External model proof includes empty FIFO and reconciled history.
                    for _ in range(read(base+m['rx'])):read(base+m['fifo'])
                    read(base+m['clear']);require(read(base+m['rx'])==read(base+m['tx'])==0,'FIFO not drained')
                    send(b'R'+bytes([copied]));execute('cont');until('WS53 SPIDMA done')
                finally:
                    if proc.poll() is None:execute('quit')
            remaining,errors=proc.communicate(timeout=5);output.extend(remaining)
            require(proc.returncode==0,f'QEMU exit {proc.returncode}');require(errors.decode().splitlines()==NOTICES,errors.decode())
            lines=['WS53 SPIDMA ready']+([] if scenario in 'CBLZ' else ['WS53 SPIDMA armed'])
            lines+=['WS53 SPIDMA retained','WS53 PASS SPIDMA retained=1 data=1','WS53 SPIDMA done']
            require(output.decode().splitlines()==lines,str(output))
            return {'port':port,'tx_channel':ch,'rx_channel':rxch,'fault_side':side,'scenario':scenario,
                    'copied_elements':copied,'pending_at_stop':pending,'abandoned':scenario in 'DF',
                    'memory_guard_address_and_raw_verified':True,'hardware_verified':False}
        finally:
            if proc.poll() is None:proc.terminate();proc.communicate(timeout=5)


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--qemu',required=True);p.add_argument('--elf',required=True,type=Path)
    a=p.parse_args();contract=elf_check.validate(a.elf)
    def digest(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
    qemu_sha256=digest(a.qemu);elf_sha256=digest(a.elf)
    with a.elf.open('rb') as stream:
        symbols=ELFFile(stream).get_section_by_name('.symtab');src=symbols.get_symbol_by_name('WS53_SPI_DMA_SOURCE');dst=symbols.get_symbol_by_name('WS53_SPI_DMA_DESTINATION')
        require(src and dst and len(src)==len(dst)==1,'missing allocation symbols');width=src[0]['st_size']//35
        require(width in (1,4) and src[0]['st_size']==dst[0]['st_size']==35*width,'wrong allocation size')
        source,destination=src[0]['st_value'],dst[0]['st_value']
    cases=[(port,ch,'t','N') for port in range(2) for ch in range(8)]
    cases += [(port,0,side,s) for port in range(2) for side in 'tr' for s in 'TRWSDFI']
    cases += [(port,0,'t',s) for port in range(2) for s in 'CBLZ']
    results=[run(a.qemu,a.elf,width,source,destination,*case) for case in cases]
    require(digest(a.qemu)==qemu_sha256 and digest(a.elf)==elf_sha256,'model or ELF changed during test')
    print(json.dumps({'elf':contract,'qemu_sha256':qemu_sha256,'elf_sha256':elf_sha256,'width':width*8,'scenarios':results,'hardware_accessed':False,
                      'physical_cache_cs_or_drain_proven':False},indent=2))


if __name__=='__main__':main()
