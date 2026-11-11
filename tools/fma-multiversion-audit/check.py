#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Audit guarded FMA multiversioning against unmodified QEMU softfloat.

Generated source copies retain QEMU copyright/license notices; no QEMU source is
moved to CLBB-owned modules. Runs offline without touching a live game process.
Requires the configured pinned QEMU build tree and GCC x86_64 ELF host.
"""
import argparse,hashlib,json,platform,re,shlex,statistics,subprocess
from pathlib import Path

ROOT=Path(__file__).resolve().parents[2]
QEMU=ROOT/'src/nb-qemu/qemu'
BUILD=QEMU/'_build/aarch64-linux-user'
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output',type=Path,default=Path('/tmp/clbb-fma-multiversion-audit'))
a=parser.parse_args();out=a.output.resolve();out.mkdir(parents=True,exist_ok=True)
if platform.system()!='Linux' or platform.machine()!='x86_64':raise SystemExit(77)
base=subprocess.check_output(['git','show','HEAD:fpu/softfloat.c'],cwd=QEMU,text=True)
current=(QEMU/'fpu/softfloat.c').read_text()
# Exact source gate: the only allowed difference is the clone-selection macro
# and its annotation. The guarded floating-point implementation stays identical.
marker='/* Preserve the generic implementation for every host.'
i=current.index(marker);j=current.index('float32 QEMU_FLATTEN QEMU_FMA_MULTIVERSION',i)
restored=current[:i]+current[j:].replace('float32 QEMU_FLATTEN QEMU_FMA_MULTIVERSION','float32 QEMU_FLATTEN',1)
assert restored==base,'Unexpected arithmetic or fallback change in softfloat.c'
(out/'base.c').write_text(base)
(out/'candidate.c').write_text(current+r'''
extern float32 clbb_fma_default(float32,float32,float32,int,float_status *) __asm__("float32_muladd.default");
extern float32 clbb_fma_fma(float32,float32,float32,int,float_status *) __asm__("float32_muladd.fma");
float32 audit_default(float32,float32,float32,int,float_status *);
float32 audit_fma(float32,float32,float32,int,float_status *);
int audit_fma_supported(void);
float32 audit_default(float32 a,float32 b,float32 c,int f,float_status *s){return clbb_fma_default(a,b,c,f,s);}
float32 audit_fma(float32 a,float32 b,float32 c,int f,float_status *s){return clbb_fma_fma(a,b,c,f,s);}
int audit_fma_supported(void){__builtin_cpu_init();return __builtin_cpu_supports("fma")!=0;}
''')
raw=subprocess.check_output(['make','-n','-W','../../fpu/softfloat.c','fpu/softfloat.o'],cwd=BUILD,text=True)
line=next(l.split(' && ',1)[1] for l in raw.splitlines() if ' && cc ' in l)
args=shlex.split(line);command=[];i=0
while i<len(args):
 if args[i] in ['-MMD','-MP','-c']:i+=1;continue
 if args[i] in ['-MT','-MF','-o']:i+=2;continue
 if args[i]==str(QEMU/'fpu/softfloat.c'):i+=1;continue
 command.append(args[i]);i+=1
command+=['-iquote',str(QEMU/'fpu')]
libs=subprocess.check_output(['pkg-config','--libs','glib-2.0'],text=True).split()
for name in ['base','candidate']:
 subprocess.run(command+['-c',str(out/(name+'.c')),'-o',str(out/(name+'.o'))],cwd=BUILD,check=True)
 subprocess.run(['cc','-shared',str(out/(name+'.o')),'-o',str(out/(name+'.so')),'-lm',*libs],check=True)
subprocess.run(['cc','-O2',str(Path(__file__).with_name('check.c')),'-ldl','-lm','-o',str(out/'check')],check=True)
assembly=subprocess.check_output(['objdump','-d',str(out/'candidate.so')],text=True)
def body(name):return assembly.split('<'+name+'>:',1)[1].split('\n\n',1)[0]
assert 'vfmadd' in body('float32_muladd.fma')
assert not re.search(r'\bv[a-z][a-z0-9]*\s',body('float32_muladd.default')),'Baseline clone contains AVX instructions'
assert not re.search(r'call.*fmaf',body('float32_muladd.fma'))
report={'qemu_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=QEMU,text=True).strip(),
 'baseline_sha256':hashlib.sha256(base.encode()).hexdigest(),'candidate_sha256':hashlib.sha256(current.encode()).hexdigest(),
 'source_gate':'floating-point operation/status/soft fallback unchanged',
 'compiler':subprocess.check_output(['cc','--version'],text=True).splitlines()[0],
 'generic_clone_no_avx':True,'fma_clone_inline_fma_no_libm_call':True,'variants':{}}
for variant in ['float32_muladd','audit_default','audit_fma']:
 result=subprocess.run([str(out/'check'),str(out/'base.so'),str(out/'candidate.so'),variant],capture_output=True,text=True)
 (out/(variant+'.log')).write_text(result.stdout+result.stderr)
 if result.returncode==77:report['variants'][variant]={'skipped':'Host has no FMA'};continue
 if result.returncode:raise RuntimeError(result.stderr)
 match=re.search(r'CORRECT \S+ (\d+) cases',result.stdout);assert match
 record={'correct_cases':int(match[1]),'result_and_all_status_bytes_match':True,'benchmarks':{}}
 for mix in [0,1]:
  rows=[tuple(map(float,m)) for m in re.findall(r'BENCH mix'+str(mix)+r' \S+ base_ns([0-9.]+) variant_ns([0-9.]+)',result.stdout)]
  assert len(rows)==7
  baseline=statistics.median(x[0]for x in rows);candidate=statistics.median(x[1]for x in rows)
  record['benchmarks']['zero_heavy'if mix else'normal']={'baseline_median_ns':baseline,'candidate_median_ns':candidate,'candidate_over_baseline':candidate/baseline}
 report['variants'][variant]=record
(out/'report.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
