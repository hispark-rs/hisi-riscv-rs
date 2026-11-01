#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""M_CLKEN1.bit4 experiment records; electrical conclusions need TX capture."""
from pathlib import Path
from elftools.elf.elffile import ELFFile

END=b'WS53 UART CLOCK OBSERVED ports=2 phases=6 restored=1\r\n'


def require(ok,why):
    if not ok:raise ValueError(why)


def inspect(path):
    with Path(path).open('rb') as stream:
        found=ELFFile(stream).get_section_by_name('.symtab').get_symbol_by_name('WS53_UART_CLOCK_STATE')
        require(found and len(found)==1 and found[0]['st_size']==224,'missing clock observation')
        a=found[0]['st_value'];require(a%4==0 and 0x20050000<=a<=0x2005ff00-224,'clock report outside application RAM')
        return dict(address=a,size=224)


def evaluate(words):
    require(len(words)==56 and all(type(v)is int and 0<=v<2**32 for v in words),'clock mailbox width')
    require(words[:3]==[0x53c10001,6,0] and words[3]&16 and words[3]==words[4]
        and words[5:8]==[1,2,4],'clock observation incomplete/not restored')
    rows=[]
    for i in range(6):
        port,phase,before,after,off,on,flushed,count=words[8+8*i:16+8*i]
        ticks=(before-after)%2**32
        require((port,phase)==(i//3,i%3) and 32000<=ticks<3200000
            and off==(words[3]&~16 if phase==1 else words[3]) and on==words[3]
            and (flushed,count)==(1,16),'clock row')
        rows.append(dict(port=port,phase=phase,marker_ticks=ticks,gated=phase==1))
    return dict(passed=True,rows=rows,initial_word=words[3],final_word=words[4],
        gate_bit=4,physical_gate_binding_verified=False,production_handoff_verified=False)
