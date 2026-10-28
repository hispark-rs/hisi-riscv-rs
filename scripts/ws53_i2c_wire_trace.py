#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Decode model pad events independently of physical packed-sample decoding."""
import statistics


def require(ok, message):
    if not ok:
        raise ValueError(message)


def decode_events(events, *, scl, sda, begin, end):
    levels = {scl: 1, sda: 1}
    starts, stops, rises = [], [], []
    for time, pad, level in events:
        if time >= end:
            break
        if pad not in levels:
            continue
        previous = levels[pad]
        levels[pad] = level
        if time < begin or previous == level:
            continue
        if pad == sda and levels[scl]:
            (stops if level else starts).append(time)
        if pad == scl and level:
            rises.append((time, levels[sda]))
    require(len(starts) == len(stops) == 1 and starts[0] < stops[0], 'model START/STOP framing')
    bits = [level for time, level in rises if starts[0] < time < stops[0]]
    times = [time for time, _ in rises if starts[0] < time < stops[0]]
    require(len(bits) == 10 and bits[8] == 1, 'model address NACK/STOP setup clocks')
    byte = sum(bit << (7-index) for index, bit in enumerate(bits[:8]))
    return dict(address_byte=byte, ack=False, scl_rises=len(bits), start_ns=starts[0], stop_ns=stops[0],
        scl_hz=1_000_000_000/statistics.mean(b-a for a,b in zip(times[:8], times[1:9])))


def analyze(path, observation, *, bounds):
    require(len(bounds) == 8, 'model GPIO boundary phase count')
    rows = [tuple(map(int, line.split(','))) for line in path.read_text().splitlines()]
    require(rows and all(len(row) == 6 for row in rows), 'model pad trace shape')
    require(all(row[0] <= after[0] for row,after in zip(rows,rows[1:])), 'model trace time order')
    require(all(level in (0,1) and drive in (0,1) and floating in (0,1) and conflict == 0
        for _,_,level,drive,floating,conflict in rows), 'model pad state/contention')
    scl, sda = (47,22) if observation['bus'] == 0 else (8,9)
    require(all(not drive or level == 0 for _,pad,level,drive,_,_ in rows if pad in (scl,sda)),
        'model I2C actively drove high')
    markers = []
    old = 0
    for time,pad,level,*_ in rows:
        if pad == 17 and old != level:
            markers.append((time,level))
            old = level
    require(len(markers) == 16 and [level for _,level in markers] == [1,0]*8, 'model marker framing')
    events = [(time,pad,level) for time,pad,level,*_ in rows]
    results = []
    for index,phase in enumerate(observation['phases']):
        begin,end = markers[2*index][0],markers[2*index+1][0]
        low = bounds[index]['inner_ticks']*1_000_000_000//32_000_000
        high = (bounds[index]['outer_ticks']*1_000_000_000+31_999_999)//32_000_000
        require(low-64 <= end-begin <= high+64, 'model GPIO window outside measured TIMER bounds')
        if phase['empty']:
            state = {scl: 1,sda: 1}
            for time,pad,level in events:
                if pad in state and time < begin:
                    state[pad] = level
            require(all(state.values()) and not any(begin <= time < end and pad in state
                and level != 1 for time,pad,level in events), 'model empty operation changed bus')
            wire = dict(empty=True)
        else:
            wire = decode_events(events,scl=scl,sda=sda,begin=begin,end=end)
            expected_byte = 0xf4 if phase['ten_bit'] else 0xab if phase['read'] else 0xaa
            require(wire['address_byte'] == expected_byte, 'model address/direction differs')
        results.append(dict(phase=index,marker_ns=end-begin,wire=wire))
    return dict(passed=True,bus=observation['bus'],scl_pad=scl,sda_pad=sda,rows=results,
        starts=7,address_nacks=7,stops=7,empty_bus_unchanged=True,open_drain_digital=True,
        analog_rise_filter_or_physical_frequency_qualified=False)


def transactions(path, *, scl, sda, begin=0, allow_open=False):
    """Decode arbitrary completed virtual transactions, including read ACKs.

    START/STOP preparation has one additional SCL rise after the last byte.
    Preserve and require that setup clock rather than accepting partial bytes.
    """
    levels = {scl:1,sda:1}
    active = None
    segments = []
    for line in path.read_text().splitlines():
        time,pad,level,drive,floating,conflict = map(int,line.split(','))
        if pad not in levels:
            continue
        require(not conflict and (not drive or level == 0), 'I2C pad contention/push-pull high')
        previous = levels[pad]
        levels[pad] = level
        if time < begin or previous == level:
            continue
        if pad == sda and levels[scl]:
            if active is not None:
                bits = active
                require(len(bits) >= 10 and len(bits)%9 == 1, 'partial I2C byte at boundary')
                require(bits[-1] != level, 'I2C boundary setup level')
                payload = bits[:-1]
                segments.append(dict(bytes=[sum(bit << (7-j) for j,bit in enumerate(payload[i:i+8]))
                    for i in range(0,len(payload),9)],
                    acks=[not payload[i] for i in range(8,len(payload),9)],
                    ending='stop' if level else 'restart'))
            else:
                require(level == 0, 'STOP without START')
            active = None if level else []
        elif pad == scl and level and active is not None:
            active.append(levels[sda])
    require(allow_open or active is None, 'unterminated I2C transaction')
    return dict(segments=segments,open_bits=active)
