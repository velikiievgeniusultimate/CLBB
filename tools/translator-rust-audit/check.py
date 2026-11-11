#!/usr/bin/env python3
"""Offline differential test of runtime Rust dispatch against pinned QEMU/C."""
import argparse,hashlib,importlib.util,json,os,pathlib,statistics,subprocess
ROOT=pathlib.Path(__file__).resolve().parents[2]
p=argparse.ArgumentParser();p.add_argument('--rust-source',type=pathlib.Path,default=ROOT/'translator-rust/src/lib.rs');p.add_argument('--output',type=pathlib.Path);args=p.parse_args();RUST_SOURCE=args.rust_source.resolve();OUT=args.output.resolve()if args.output else ROOT/'logs/translator-rust-pac';OUT.mkdir(exist_ok=not bool(args.output))
spec=importlib.util.spec_from_file_location('pac_audit',ROOT/'tools/pauth-audit/audit.py');a=importlib.util.module_from_spec(spec);spec.loader.exec_module(a)
def run(args):return subprocess.check_output(args,text=True)
source=a.SOURCE.read_text();oracle=run(['git','-C',str(a.QEMU),'show',a.PIN+':target/arm/pauth_helper.c'])
assert a.digest(oracle)==a.ORACLE_SHA
prelude='''#define _POSIX_C_SOURCE 200809L
#include <stdint.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <tmmintrin.h>
#define MAKE_64BIT_MASK(s,n) ((UINT64_MAX >> (64 - (n))) << (s))
static inline uint64_t extract64(uint64_t x,unsigned s,unsigned n){return (x>>s)&(UINT64_MAX>>(64-n));}
static inline uint32_t extract32(uint32_t x,unsigned s,unsigned n){return (x>>s)&(UINT32_MAX>>(32-n));}
typedef struct {uint64_t lo,hi;} ARMPACKey;
'''
unit=OUT/'oracle.c';unit.write_text(prelude+a.extract(oracle,'oracle')+a.extract(source,'production')+'\n#define CLBB_PAC_C_REFERENCE 1\n'+a.extract(source,'reference')+(ROOT/'tools/translator-rust-audit/harness.c').read_text())
obj=OUT/'pac.o';cmd=['rustc','--edition=2021','--crate-type','lib','--emit=obj','-C','opt-level=3','-C','panic=abort','-C','relocation-model=pic',str(RUST_SOURCE),'-o',str(obj)];subprocess.run(cmd,check=True)
assert not run(['nm','-u',str(obj)]).strip(),'Rust object introduces runtime/panic dependencies'
affinity=sorted(os.sched_getaffinity(0));os.sched_setaffinity(0,{affinity[-1]})
report={'oracle_commit':a.PIN,'oracle_sha256':a.digest(oracle),'c_source_sha256':a.digest(source),'rust_source_sha256':a.digest(RUST_SOURCE.read_text()),'rustc':run(['rustc','--version']).strip(),'cc':run(['cc','--version']).splitlines()[0],'cpu_affinity':[affinity[-1]],'rust_compile':cmd,'unresolved_symbols':[],'results':{}}
for opt in ('O2','O3'):
 exe=OUT/('check-'+opt);subprocess.run(['cc','-std=c11','-'+opt,str(unit),str(obj),'-o',str(exe)],check=True)
 raw=run([str(exe)]);(OUT/(opt+'.jsonl')).write_text(raw);lines=[json.loads(l) for l in raw.splitlines()];result={'correctness':lines[0],'medians':{}}
 for lanes in (1,4):
  samples=[l for l in lines[1:] if l['lanes']==lanes];c=statistics.median(l['c_ns'] for l in samples);r=statistics.median(l['rust_ns'] for l in samples);result['medians'][str(lanes)]={'c_ns':c,'rust_ns':r,'rust_over_c':r/c}
 report['results'][opt]=result;print(opt,result,flush=True)
report['performance_gate']={'max_median_rust_over_c':1.02,'scope':'full-dispatch CPU microbenchmark, not whole startup'}
assert all(m['rust_over_c']<=1.02 for result in report['results'].values() for m in result['medians'].values()), 'Rust PAC dispatch performance regressed beyond 2% noise allowance'
report['pass']=True;(OUT/'report.json').write_text(json.dumps(report,indent=2)+'\n')
