#!/usr/bin/env python3
"""Compile the production TCG-size parser with real env; never start QEMU/game."""
from pathlib import Path
import os
import subprocess
import tempfile

ROOT=Path(__file__).resolve().parents[1]
production=(ROOT/'src/nb-qemu/qemu/linux-user/main.c').read_text()
start=production.index('static unsigned long clbb_tcg_cache_bytes(void)')
end=production.index('{',start)+1;depth=1
while depth:
    depth+=(production[end]=='{')-(production[end]=='}');end+=1
helper=production[start:end]
assert production.count('tcg_exec_init(clbb_tcg_cache_bytes());')==1
assert 'tcg_exec_init(0);' not in production
code=r'''
#include <assert.h>
#include <errno.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include "qemu/units.h"
''' + helper + r'''
int main(void){
 errno=EDOM;unsigned long value=clbb_tcg_cache_bytes();assert(errno==EDOM);
 errno=E2BIG;assert(clbb_tcg_cache_bytes()==value);assert(errno==E2BIG);
 printf("%lu\n",value);
}
'''
valid={'32':32,'33':33,'128':128,'240':240,'512':512,'1023':1023,'1024':1024,
       '000512':512,'0'*1000+'512':512}
invalid=[None,'','0','1','31','1025','2048','-32','-512','+512',' 512','512 ',
         '512\n','\t512','512MiB','512M','0x200','5.12','1e3','１２８','٥١٢',
         '4294967296','18446744073709551615','18446744073709551616','9'*10000]
with tempfile.TemporaryDirectory(prefix='clbb-tcg-size-') as temporary:
    directory=Path(temporary);source=directory/'check.c';binary=directory/'check'
    source.write_text(code)
    subprocess.run(['cc','-std=c11','-O2','-Wall','-Wextra','-Werror',
                    '-fsanitize=undefined','-fno-sanitize-recover=all',
                    '-I'+str(ROOT/'src/nb-qemu/qemu/include'),str(source),'-o',str(binary)],check=True)
    checks=0
    for value,expected in [*valid.items(),*((value,0) for value in invalid)]:
        env=dict(os.environ);env.pop('CLBB_TCG_CACHE_MIB',None)
        if value is not None:env['CLBB_TCG_CACHE_MIB']=value
        result=subprocess.run([str(binary)],env=env,capture_output=True,text=True,check=True,timeout=3)
        assert int(result.stdout)==expected*1024*1024,(value,result.stdout)
        checks+=1
print(f'TCG_CACHE_SIZE_OK cases={checks} actual_production_helper=1 real_environment=1 errno_preserved=1 ubsan=1 no_runtime_launch=1')
