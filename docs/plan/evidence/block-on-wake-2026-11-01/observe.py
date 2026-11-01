#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Observe a known block_on defect; this is NOT a passing qualification gate."""
import argparse
import hashlib
import json
import re
from pathlib import Path
import subprocess
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--qemu',type=Path,required=True)
p.add_argument('--counterexample',type=Path,required=True)
p.add_argument('--control',type=Path,required=True)
p.add_argument('--output',type=Path,required=True)
a=p.parse_args()
if a.output.exists():
    raise FileExistsError(a.output)
results={}
for name,elf in [('counterexample',a.counterexample),('control',a.control)]:
    command=[str(a.qemu.resolve()),'-M','ws63','-display','none','-monitor','none','-serial','stdio','-kernel',str(elf.resolve()),'-icount','shift=0,sleep=on','-d','guest_errors']
    proc=subprocess.Popen(command,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    timeout=False
    try:
        stdout,stderr=proc.communicate(timeout=3)
    except subprocess.TimeoutExpired:
        timeout=True
        proc.terminate()
        stdout,stderr=proc.communicate(timeout=3)
    results[name]={'command':command,'stdout':stdout.decode(errors='replace'),'stderr':stderr.decode(errors='replace'),'returncode':proc.returncode,'terminated_after_window':timeout,'elf_sha256':hashlib.sha256(elf.read_bytes()).hexdigest()}
for result in results.values():
    result['unexpected_diagnostics'] = [line for line in result['stderr'].splitlines()
        if not line.startswith('ws63-uart: baud divider set ')
        and not re.fullmatch(r'qemu-system-riscv32: terminating on signal 15 from pid \d+ .*', line)]
results['counterexample_reproduced']=(all('WAKE TEST: WAKE ISSUED' in results[n]['stdout'] for n in ['counterexample','control']) and 'WAKE TEST: COMPLETE' not in results['counterexample']['stdout'] and 'WAKE TEST: COMPLETE' in results['control']['stdout'])
results['counterexample_reproduced'] &= all(not results[name]['unexpected_diagnostics'] and results[name]['terminated_after_window'] for name in ['counterexample','control'])
results.update(hardware_accessed=False,qualification_passed=False,defect_fixed=False,qemu_sha256=hashlib.sha256(a.qemu.read_bytes()).hexdigest())
a.output.write_text(json.dumps(results,indent=2)+'\n')
print(json.dumps(results,indent=2))
if not results['counterexample_reproduced']:
    raise SystemExit('Counterexample not reproduced; inspect actual behavior, do not infer success.')
