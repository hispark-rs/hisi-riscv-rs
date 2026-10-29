#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Decode actual routed model pads, independently of FIFO/register completion."""
from bisect import bisect_right
from pathlib import Path
import statistics
from ws53_spi_wire import require


def analyze(path, observation):
    pads = {}
    for line in Path(path).read_text().splitlines():
        row = list(map(int, line.split(',')))
        require(len(row) == 6 and row[0] >= 0 and 0 <= row[1] < 48
            and all(value in (0,1) for value in row[2:]), 'SPI model trace record')
        time, pad, *state = row
        values = pads.setdefault(pad, [])
        require(not values or time >= values[-1][0], 'SPI trace time order')
        values.append((time, *state))
    require(all(pad in pads for pad in (8,9,12,17,36)), 'SPI model physical route absent')
    times = {pad:[row[0] for row in values] for pad,values in pads.items()}

    def state(pad, time):
        index = bisect_right(times[pad], time)-1
        require(index >= 0, 'SPI model unobserved initial pad')
        return pads[pad][index][1:]

    def edges(pad, begin, end):
        old = state(pad, begin-1)[0] if begin else pads[pad][0][1]
        result = []
        for time,level,*_ in pads[pad]:
            if not begin <= time < end:
                continue
            if level != old:
                result.append((time,level))
            old = level
        return result

    end = max(row[0] for values in pads.values() for row in values)+1
    marker = edges(17,0,end)
    require(len(marker) == 32 and [level for _,level in marker] == [1,0]*16, 'SPI model marker lifecycle')
    rows = []
    for index,phase in enumerate(observation['phases']):
        begin,finish = marker[2*index][0],marker[2*index+1][0]
        duration = finish-begin
        require(phase['inner_ticks']*1_000_000_000//32_000_000-66 <= duration
            <= (phase['outer_ticks']*1_000_000_000+31_999_999)//32_000_000+66,
            'SPI model GPIO/TIMER measured bounds')
        ce = edges(36,begin,finish)
        require(len(ce)==2 and [value for _,value in ce]==[0,1]
            and state(36,begin)[0]==state(36,finish-1)[0]==1, 'SPI model software CS')
        start,stop = ce[0][0],ce[1][0]
        clocks = edges(8,start,stop)
        cpol,cpha = phase['mode']//2,phase['mode']%2
        bits,count = phase['bits'],phase['count']
        require(len(clocks)==2*bits*count and [level for _,level in clocks]==[1-cpol,cpol]*(bits*count)
            and state(8,start)[0]==state(8,stop-1)[0]==cpol,'SPI model SCK count/polarity')
        for pad in (8,9,12,36):
            require(not any(state(pad,start)[2:]) and
                all(not floating and not conflict for time,level,drive,floating,conflict in pads[pad]
                if start<=time<stop), 'SPI model floating/conflicting selected pad')
        require(all(state(pad,start)[1] for pad in (8,9,36)), 'SPI model output not driven')
        sampled = []
        for time,_ in clocks[cpha::2]:
            value = state(9,time)[0]
            require(state(9,time-1)[0]==value==state(9,time+1)[0], 'SPI model MOSI setup/hold')
            sampled.append(value)
        words = [sum(value<<(bits-1-i) for i,value in enumerate(sampled[offset:offset+bits]))
            for offset in range(0,len(sampled),bits)]
        require(words==phase['tx'], 'SPI model wire payload differs')
        require(state(12,start)[0]==int(phase['miso_high']) and all(level==int(phase['miso_high'])
            for time,level,*_ in pads[12] if start<=time<stop), 'SPI model static MISO differs')
        periods = []
        for offset in range(0,len(clocks),2*bits):
            samples = clocks[offset:offset+2*bits][cpha::2]
            periods += [b[0]-a[0] for a,b in zip(samples,samples[1:])]
        require(periods and min(periods)>0, 'SPI model period absent')
        rows.append(dict(phase=index,mode=phase['mode'],bits=bits,words=words,
            clock_edges=len(clocks),sample_edges=len(sampled),marker_ns=duration,
            period_ns=periods,measured_hz=1_000_000_000/statistics.mean(periods)))
    return dict(passed=True,rows=rows,frames=96,bits=1536,cpol_cpha_verified=True,
        routed_static_miso_verified=True,native_cs_verified=False,analog_timing_verified=False)
