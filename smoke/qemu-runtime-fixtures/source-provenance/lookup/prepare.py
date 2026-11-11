#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Private GPL QEMU source-copy experiment. No canonical source mutation."""
import hashlib,json,pathlib,sys
R=pathlib.Path(__file__).resolve().parents[2];H=pathlib.Path(__file__).resolve().parent
S=R/'src/nb-qemu/qemu/accel/tcg/tcg-runtime.c'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def body():
 text=S.read_text();start=text.index('void *HELPER(lookup_tb_ptr)(CPUArchState *env)');end=text.index('\nvoid HELPER(exit_atomic)',start)
 return text,text[start:end].rstrip()
def prepare(out):
 out.mkdir(parents=True,exist_ok=False);text,original=body()
 slow=original.replace('void *HELPER(lookup_tb_ptr)','static void * __attribute__((noinline)) clbb_lookup_tb_ptr_slow',1)
 wrapper=(H/'wrapper.inc').read_text()
 assert wrapper.count('void *HELPER(lookup_tb_ptr)')==1
 changed=text.replace(original,slow+'\n\n'+wrapper.rstrip(),1)
 assert changed.replace(slow+'\n\n'+wrapper.rstrip(),original,1)==text
 (out/'tcg-runtime.c').write_text(changed)
 (out/'lookup-implementations.inc').write_text(original.replace('void *HELPER(lookup_tb_ptr)','static void * __attribute__((noinline)) baseline_lookup',1)+'\n'+slow+'\n'+wrapper.replace('void *HELPER(lookup_tb_ptr)','static void * __attribute__((noinline)) candidate_lookup',1))
 (out/'candidate.patch').write_text(__import__('subprocess').run(['diff','-u','--label','a/accel/tcg/tcg-runtime.c','--label','b/accel/tcg/tcg-runtime.c',str(S),str(out/'tcg-runtime.c')],capture_output=True,text=True).stdout)
 report=dict(schema=1,pass_=True,input_path=str(S),input_sha256=sha(S),wrapper_sha256=sha(H/'wrapper.inc'),output_sha256=sha(out/'tcg-runtime.c'),original_slow_body_exact=True,inverse_byte_identical=True,attribution='Derived from licensed QEMU existing GPL full-key lookup and ARM state extraction; original source notices retained. No independent ownership claim.')
 report['pass']=report.pop('pass_');(out/'prepare-report.json').write_text(json.dumps(report,indent=2)+'\n')
if __name__=='__main__':prepare(pathlib.Path(sys.argv[1]))
