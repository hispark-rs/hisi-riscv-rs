#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Actual Rust/PAC I2C DMA command and receive ownership tests in QEMU; no hardware."""
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

NOTICES=['ws53-mdma: assumed DTCM mem-to-mem same-width posted-write profile',
         'ws53-mdma: assumed I2C0/1 byte-data halfword-command requests, burst1/4, TXprot1/RXprot2',
         'ws53-i2c: assumed profile 32MHz FIFO8 CON-addressing deferred-read-ACK disable=1us']


def run(qemu,elf,source,destination,port,ch,kind,burst,side,scenario):
    wire_kind=kind;ten=kind.isupper();kind=kind.lower()
    rxch=(ch+1)%8;target=rxch if side=='r' else ch;normal=scenario in 'N134'
    length=33 if scenario=='N' else int(scenario) if normal else 5
    reads=0 if kind=='w' else length-1 if kind=='c' else length;base=0x52083000+port*0x1000
    payload=[0x20]+[(n*7+ch+port*29)&255 for n in range(1,length)]
    pattern=[(payload[n] if kind=='w' else 0x20 if kind=='c' and n==0 else 0x100)
             |(0x400 if kind=='c' and n==1 else 0)|(0x200 if n==length-1 else 0) for n in range(length)]
    with tempfile.TemporaryDirectory(prefix='ws53-i2c-dma-hal-') as temp:
        work=Path(temp);qtpath=work/'qtest';qmpath=work/'qmp'
        args=[qemu,'-M','ws53','-S','-display','none','-monitor','none','-kernel',str(elf),
              '-icount','shift=0,sleep=on','-global','ws53-mdma.assume-profile=on',
              '-global','ws53-mdma.assume-i2c=on','-global','ws53-i2c.assume-profile=on',
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
                def i2c_fault(name,on=True):cmd(f'set_irq_in /machine/i2c {name} {port} {int(on)}')
                def send(s):proc.stdin.write(s);proc.stdin.flush()
                def advance_until(condition):
                    execute('cont');deadline=time.monotonic()+5
                    while not condition():
                        require(time.monotonic()<deadline,f'{burst}/{port}/{ch}/{scenario}: model setup did not settle');time.sleep(.002)
                    execute('stop')
                def until(line):
                    deadline=time.monotonic()+10
                    while (line+'\n').encode() not in output:
                        require(time.monotonic()<deadline,f'{burst}/{port}/{ch}/{side}/{scenario}: timeout {output!r}')
                        for key,_ in selector.select(.02):
                            chunk=os.read(key.fd,4096);require(chunk,'guest exit');output.extend(chunk)
                def words(addr,width):
                    raw=bytes.fromhex(cmd(f'read {addr:#x} {35*width:#x}')[0].decode().removeprefix('0x'))
                    return [int.from_bytes(raw[n:n+width],'little') for n in range(0,len(raw),width)]
                def check_memory(copied):
                    require(words(source,2)==[0x5a]+pattern+[0x5a]*(34-length),'command/guard mismatch')
                    expected=[0x5a]+[(((0x20 if kind=='c' else 0)+n)^0xa5) if n<copied else 0x5a for n in range(reads)]+[0x5a]*(34-reads)
                    require(words(destination,1)==expected,'destination/guard mismatch')
                execute('qmp_capabilities')
                try:
                    execute('cont');until('WS53 I2CDMA ready');execute('stop')
                    gate=int(cmd('readw 0x52000044')[0],0);cmd(f'writew 0x52000044 {gate|(0x40<<port):#x}')
                    write(base,0x263|(16 if ten else 0));write(base+0x10,0x250 if ten else 0x50);write(base+0x30,153);write(base+0x34,159);write(base+4,1)
                    advance_until(lambda:read(base+0x84)==1)
                    if scenario in 'TDFI':fault('bus-stall')
                    if scenario=='S':fault('stall')
                    if scenario=='R':fault('read-error')
                    if scenario=='W':fault('write-error')
                    if scenario=='C':write(base+4,0);advance_until(lambda:read(base+0x84)==0)
                    if scenario=='B':i2c_fault('stretch');write(base+0x1c,0x100)
                    if scenario=='L':
                        i2c_fault('nack-address');write(base+0x1c,0x100);advance_until(lambda:read(base+0xb8)&64);i2c_fault('nack-address',False)
                    if scenario=='A':i2c_fault('nack-address')
                    send(bytes([48+port,48+ch])+wire_kind.encode()+str(burst).encode()+scenario.encode());execute('cont')
                    configured=scenario not in 'CBL'
                    if configured:
                        until('WS53 I2CDMA armed')
                        channels=[(ch,False)]+([(rxch,True)] if reads else [])
                        for channel,rx in channels:
                            cfg=reg(channel,8);ctrl=reg(channel,0x14);request=port*2+int(rx)
                            require((cfg>>9)&7==(2 if rx else 1) and (cfg>>(1 if rx else 5))&15==request,'route mismatch')
                            require((ctrl>>28)&7==(2 if rx else 1),'protection mismatch')
                            require((ctrl>>18)&7==int(not rx) and (ctrl>>21)&7==int(not rx),'width mismatch')
                            require((ctrl>>12)&7==int(burst==4) and (ctrl>>15)&7==int(burst==4),'burst mismatch')
                        send(b'G')
                    until('WS53 I2CDMA retained');execute('stop')
                    pending=dma('pending-writes');require(pending==int(scenario in 'TDFI'),'pending beat mismatch')
                    require(read(base+0xa0)==((3 if reads else 2) if scenario=='F' else 0),'requests mismatch')
                    if scenario!='F':require(read(0x52070010)==0,'DMA still enabled after stop')
                    before=reads-(reg(rxch,0x14)&0xfff) if configured and reads else 0;check_memory(before)
                    if normal:
                        require(prop('/machine/i2c',f'stops{port}')==1 and not read(base+0x60)&1,'STOP missing')
                        require(prop('/machine/i2c',f'read-acks{port}')==max(reads-1,0),'ACK count')
                        require(prop('/machine/i2c',f'read-nacks{port}')==int(reads>0),'NACK count')
                        require(prop('/machine/i2c',f'restarts{port}')==int(kind=='c'),'repeated START count')
                        if burst==4 and reads%4:require(read(base+0xa8)==0,'short tail watermark not lowered')
                    # Independent external model recovery, including forgotten
                    # transfers. Halt all configured channels before unblocking a
                    # posted beat; no physical drain guarantee is inferred.
                    if configured:
                        write(base+0xa0,0)
                        for channel,_ in channels:write(0x52070108+channel*32,(reg(channel,8)&~1)|0x10000)
                    if scenario in 'TDFI':fault('bus-stall',False)
                    if scenario=='S':fault('stall',False)
                    if scenario=='R':fault('read-error',False)
                    if scenario=='W':fault('write-error',False)
                    if scenario=='A':i2c_fault('nack-address',False)
                    if scenario=='B':i2c_fault('stretch',False)
                    advance_until(lambda:dma('pending-writes')==0 and read(0x52070010)==0)
                    copied=reads-(reg(rxch,0x14)&0xfff) if configured and reads else 0
                    expected=reads if normal else 1 if side=='r' and scenario in 'TDFI' else 0
                    require(copied==expected,f'RX progress {copied} != {expected}');check_memory(copied)
                    if configured:
                        sent=length-(reg(ch,0x14)&0xfff)
                        require(reg(ch,0x10)==source+2+sent*2 and reg(ch,4)==base+0x1c,'TX progress')
                        if reads:require(reg(rxch,0x10)==base+0x1c and reg(rxch,4)==destination+1+copied,'RX progress')
                        raw=read(0x5207000c)
                        require(raw>>8==((1<<target) if scenario in 'RW' else 0),'DMA raw error')
                        require(raw&255==(int(sent==length)<<ch)|((int(copied==reads)<<rxch) if reads else 0),'DMA raw TC')
                    # Independently read the virtual peer back for pure writes.
                    if normal and kind=='w':
                        write(base+0xc4,1);write(base+0x1c,0x20);actual=[]
                        for begin in range(0,length-1,7):
                            count=min(7,length-1-begin)
                            for n in range(begin,begin+count):
                                write(base+0x1c,(0x500 if n==0 else 0x100)|(0x200 if n==length-2 else 0))
                            advance_until(lambda:read(base+0x68)==count)
                            actual.extend(read(base+0x1c) for _ in range(count))
                        require(actual==payload[1:],'peer write/readback mismatch')
                    write(base+4,0);advance_until(lambda:read(base+0x84)==0)
                    write(base+0xc4,1)
                    require(read(base+0x64)==read(base+0x68)==0 and not read(base+0x60)&1,'bus/FIFO recovery failed')
                    send(b'R'+bytes([copied]));execute('cont');until('WS53 I2CDMA done')
                finally:
                    if proc.poll() is None:execute('quit')
            remaining,errors=proc.communicate(timeout=5);output.extend(remaining)
            require(proc.returncode==0,f'QEMU exit {proc.returncode}');require(errors.decode().splitlines()==NOTICES,errors.decode())
            lines=['WS53 I2CDMA ready']+(['WS53 I2CDMA armed'] if configured else [])
            lines+=['WS53 I2CDMA retained','WS53 PASS I2CDMA retained=1 data=1','WS53 I2CDMA done']
            require(output.decode().splitlines()==lines,str(output))
            return {'port':port,'tx_channel':ch,'kind':kind,'address_bits':10 if ten else 7,'burst':burst,'fault_side':side,'scenario':scenario,
                    'copied_bytes':copied,'pending_at_stop':pending,'abandoned':scenario in 'DF',
                    'memory_guards_commands_progress_and_raw_verified':True,'hardware_verified':False}
        finally:
            if proc.poll() is None:proc.terminate();proc.communicate(timeout=5)


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--qemu',required=True);p.add_argument('--elf',required=True,type=Path)
    a=p.parse_args();contract=elf_check.validate(a.elf)
    with a.elf.open('rb') as stream:
        symbols=ELFFile(stream).get_section_by_name('.symtab');src=symbols.get_symbol_by_name('WS53_I2C_DMA_SOURCE');dst=symbols.get_symbol_by_name('WS53_I2C_DMA_DESTINATION')
        require(src and dst and len(src)==len(dst)==1,'missing allocations');require(src[0]['st_size']==70 and dst[0]['st_size']==35,'allocation size')
        source,destination=src[0]['st_value'],dst[0]['st_value']
    cases=[(port,ch,kind,burst,'t','N') for port in range(2) for ch in range(8) for kind in 'rwc' for burst in (1,4)]
    cases += [(port,0,kind,burst,'t','N') for port in range(2) for kind in 'RWC' for burst in (1,4)]
    cases += [(port,0,'r',4,'t',s) for port in range(2) for s in '134']
    cases += [(port,0,'r',1,side,s) for port in range(2) for side in 'tr' for s in 'TRWSDFIA']
    cases += [(port,0,'w',1,'t',s) for port in range(2) for s in 'TRWSDFI']
    cases += [(port,0,'r',1,'t',s) for port in range(2) for s in 'CBL']
    results=[run(a.qemu,a.elf,source,destination,*case) for case in cases]
    print(json.dumps({'elf':contract,'scenarios':results,'hardware_accessed':False,
                      'physical_burst_cache_or_drain_proven':False},indent=2))


if __name__=='__main__':main()
