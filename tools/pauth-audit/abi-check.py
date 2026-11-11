#!/usr/bin/env python3
"""Read-only production QEMU helper/call-clobber contract for PAC's XMM use."""
from pathlib import Path
import json,hashlib,re
p=Path(__file__).resolve().parent;r=p.parents[1];q=r/'src/nb-qemu/qemu'
files={name:(q/path).read_text()for name,path in {'helper':'target/arm/helper-a64.h','target':'tcg/i386/tcg-target.inc.c','allocator':'tcg/tcg.c','flags':'include/tcg/tcg.h'}.items()}
for name in ['pacia','pacib','pacda','pacdb','pacga','autia','autib','autda','autdb']:
 assert f'DEF_HELPER_FLAGS_3({name}, TCG_CALL_NO_WG, i64, env, i64, i64)'in files['helper']
assert 'tcg_target_call_clobber_regs = ALL_VECTOR_REGS;'in files['target']
a=files['allocator'];start=a.index('static void tcg_reg_alloc_call(');body=a[start:];clobber=body.index('/* clobber call registers */');sync=body.index('/* Save globals if they might be written');call=body.index('tcg_out_call(s, func_addr);');assert clobber<sync<call
assert 'tcg_reg_free(s, i, allocated_regs);'in body[clobber:sync]
assert '#define TCG_CALL_NO_WG          TCG_CALL_NO_WRITE_GLOBALS'in files['flags']
free=a[a.index('static void tcg_reg_free('):];free=free[:free.index('\n}\n')+3];assert 'temp_sync('in free
result={'pass':True,'meaning':'NO_WG describes guest-global writes; every ordinary helper call still clobbers host vector registers and spills live TCG values. XMM use is ABI-correct.','source_sha256':{key:hashlib.sha256(value.encode()).hexdigest()for key,value in files.items()}}
(p/'artifacts/abi-source-contract.json').write_text(json.dumps(result,indent=2)+'\n');print('PAUTH_XMM_CALL_CLOBBER_CONTRACT_PASS')
