#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Exercise constructor failures with a synthetic DLL; no USB or SEGGER library."""
import json, os, subprocess, tempfile
from pathlib import Path
root=Path(__file__).resolve().parents[1]
commands=['DisableAutoUpdateFW','DisableFlashDL','DisableFlashBPs']
rows=[]
with tempfile.TemporaryDirectory(prefix='ws53-guard-test-') as directory:
    d=Path(directory); guard=d/'guard.so'; fake=d/'fake.so'; empty=d/'empty.so'
    source=d/'fake.c'
    source.write_text('''#include <stdlib.h>
#include <string.h>
#include <stdio.h>
int JLINKARM_ExecCommand(const char *cmd, char *error, int size) {
 (void)error; (void)size;
 const char *fail=getenv("MOCK_REJECT");
 fprintf(stderr,"MOCK %s\\n",cmd);
 return fail && strcmp(fail,cmd)==0 ? -1 : 0;
}
''')
    (d/'empty.c').write_text('int unrelated_symbol(void) { return 0; }\n')
    for src,out in [(root/'hil/jlink-no-update-guard.c',guard),(source,fake),(d/'empty.c',empty)]:
        subprocess.run(['cc','-shared','-fPIC','-Wall','-Wextra','-Werror','-o',str(out),str(src),'-ldl'],check=True)
    cases=[('missing-path',None,None,[]),('missing-library',str(d/'absent.so'),None,[]),
           ('missing-command',str(empty),None,[]),('success',str(fake),None,commands)]
    cases += [(f'rejected-{i}',str(fake),cmd,commands[:i+1]) for i,cmd in enumerate(commands)]
    for name,lib,reject,expected in cases:
        env=os.environ.copy(); env.pop('WS53_JLINK_LIBRARY',None); env.pop('MOCK_REJECT',None)
        env['LD_PRELOAD']=str(guard)
        if lib is not None: env['WS53_JLINK_LIBRARY']=lib
        if reject is not None: env['MOCK_REJECT']=reject
        result=subprocess.run(['/usr/bin/true'],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=5)
        actual=[line.removeprefix('MOCK ') for line in result.stderr.decode().splitlines() if line.startswith('MOCK ')]
        assert result.returncode==(0 if name=='success' else 125), (name,result.stderr)
        assert actual==expected,(name,actual,expected)
        rows.append(dict(case=name,exit_code=result.returncode,commands=actual))
print(json.dumps(dict(hardware_accessed=False,cases=rows),indent=2))
