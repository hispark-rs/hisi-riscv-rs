#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""SPI0 physical wire observations with static MISO; not an external slave."""
from pathlib import Path
from elftools.elf.elffile import ELFFile

END = b'WS53 SPI WIRE OBSERVED phases=16 restored=1\r\n'
TX8 = [0x96, 0x3c, 0xa5, 0, 0xff, 0x81, 0x7e, 0x42]
TX32 = [0x963ca500, 0xff817e42, 0x13579bdf, 0x02468ace]


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def inspect(path):
    with Path(path).open('rb') as file:
        elf = ELFFile(file)
        rows = elf.get_section_by_name('.symtab').get_symbol_by_name('WS53_SPI_WIRE')
        require(rows and len(rows) == 1 and rows[0]['st_size'] == 1408, 'SPI wire report symbol')
        address = rows[0]['st_value']
        require(address % 4 == 0 and 0x20050000 <= address <= 0x2005ff00-1408, 'SPI report outside owned DTCM')
    return dict(address=address, size=1408, bus=0)


def evaluate(words):
    require(len(words) == 352 and all(type(value) is int and 0 <= value < 2**32 for value in words),
        'SPI wire report width/type')
    require(words[:4] == [0x53c40001, 16, 1, 0], 'SPI wire incomplete/failed/version')
    require(words[4:7] == words[7:10] and all(value <= 0xffff for value in words[4:10]), 'SPI clock/mode not restored')
    require(words[10:12] == words[12:14] and words[14:19] == [1, 0, 0, 0, 0], 'SPI pads/IRQ/DMA/enable not restored')
    # SPI0 v151_100 WSR differs from QSPI1 SR: BUSY15, TFE12, TFNF11,
    # RFF5, RFNE4 and collision0. RDL/PAC 04ced9c, SDK spi_wsr_data_t.
    require(words[19] & 0x9831 == 0x1800 and words[20:32] == [0]*12, 'SPI busy/FIFO/error/reserved header')
    phases = []
    for index in range(16):
        row = words[32+index*20:52+index*20]
        bits, count = (8, 8) if index < 8 else (32, 4)
        high = (index+index//8) % 2
        require(row[:6] == [index, bits, index % 4, 500_000 if (index//4)%2 else 100_000, high, 0]
            and row[10] == count, 'SPI phase/config/failure')
        expected = (2**bits-1) if high else 0
        require(row[11:11+count] == [expected]*count and row[11+count:19] == [0]*(8-count)
            and row[19] & 0x9831 == 0x1800, 'SPI receive/padding/busy/FIFO/error')
        t0, t1, t2, t3 = row[6:10]
        left, inner, right = ((a-b) % 2**32 for a, b in ((t0,t1), (t1,t2), (t2,t3)))
        require(left > 0 and inner > 0 and right > 0 and left+inner+right == (t0-t3) % 2**32 < 32_000_000,
            'SPI missing/unordered GPIO timestamps')
        phases.append(dict(phase=index, bits=bits, mode=index % 4, requested_hz=row[3], miso_high=bool(high),
            count=count, tx=TX8 if bits == 8 else TX32, rx=row[11:11+count],
            inner_ticks=inner, outer_ticks=left+inner+right))
    return dict(passed=True, bus=0, phases=phases, clocks_restored=True, pads_restored=True,
        external_slave_verified=False, physical_frequency_parity_proven=False,
        qspi_verified=False, flash_qualified=False, cold_boot_verified=False)
