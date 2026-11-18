#!/usr/bin/env python3
"""Emit public AArch64 instruction tests and independent unbounded-integer oracles."""
from pathlib import Path
import sys

out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
U64 = (1 << 64) - 1
cases = [
    (0,0),(0,1),(1,0),(7,7),(31,33),(33,33),(4095<<12,0),
    (0x7fffffff,0xffffffff),(0x80000000,1),(0xffffffff,0x80000000),
    (0xdecafbad00000000,0xbeef000000000001),(0xfeed000080000000,0xabcd00007fffffff),
    ((1<<63)-1,U64),(1<<63,1),(U64,1<<63),(0x13579bdf2468ace0,0xfdb97531eca86420)
]
sources = {
    'xr': 'cmp x0, x1', 'wr': 'cmp w0, w1', 'xi': 'cmp x0, #33',
    'wi': 'cmp w0, #4095, lsl #12', 'xz': 'cmp xzr, x1', 'sp': 'cmp sp, #16',
    'shift': 'cmp w0, w1, lsl #1',
    'gap': 'cmp x0, x1\n    mov x0, #11\n    mov x1, #4',
    'replace': 'cmp x0, x1\n    cmp x1, #7',
    'neutral': 'cmp x0, x1\n    add x6, x4, #11\n    eor x7, x4, x5\n    mov x8, x6',
    'neutralw': 'cmp w0, w1\n    eor w6, w0, w1',
    'sourcewrite': 'cmp x0, x1\n    add x0, x0, #1',
    'flagwrite': 'cmp x0, x1\n    adds x6, x4, #1',
    'mem': 'cmp x0, x1\n    str x4, [x2]\n    ldr x6, [x2]',
    'outside': 'cmp x0, x1\n    add x6,x4,#1\n    add x6,x6,#3\n    eor x7,x6,x5\n    mov x8,x7',
    'entry': 'cmp x0, x1\n    b 1f\n1:\n    mov x0, #11\n    mov x1, #4',
}
conditions = ('eq','ne','hs','lo','mi','pl','vs','vc','hi','ls','ge','lt','gt','le')
ops = ('csel','cset','csetm','cinc','cneg','cinv','branch','ccmp','ccmn','fcsel')

def flags(a,b,n,sub=True):
    m=1<<n; a%=m; b%=m
    sa=a-m if a>>(n-1) else a
    sb=b-m if b>>(n-1) else b
    raw=(a-b if sub else a+b)
    signed=(sa-sb if sub else sa+sb)
    neg,zero,carry,over=(raw%m)>>(n-1), raw%m==0, a>=b if sub else raw>=m, not -(m//2)<=signed<m//2
    return (neg<<3)|(zero<<2)|(carry<<1)|over

def test_cond(f,i):
    n,z,c,v=(bool(f & b) for b in (8,4,2,1))
    return (z,not z,c,not c,n,not n,v,not v,c and not z,not c or z,n==v,n!=v,not z and n==v,z or n!=v)[i]

def result(src,op,cc,a,b):
    vals={'xr':(a,b,64),'wr':(a,b,32),'xi':(a,33,64),'wi':(a,4095<<12,32),
          'xz':(0,b,64),'sp':(4096,16,64),'shift':(a,b<<1,32),'gap':(a,b,64),
          'replace':(b,7,64),'entry':(a,b,64),'neutral':(a,b,64),'neutralw':(a,b,32),
          'sourcewrite':(a,b,64),'flagwrite':(7,1,64),'mem':(a,b,64),'outside':(a,b,64)}
    f=flags(*vals[src],sub=src!='flagwrite'); yes=test_cond(f,cc)
    if src=='sourcewrite': a=(a+1)&U64
    if src in ('gap','entry'):
        a,b=11,4
    if op=='ccmp': return 0, flags(a,b,64) if yes else 5
    if op=='ccmn': return 0, flags(a,3,32,False) if yes else 11
    truth, false={'csel':(7,U64-4),'cset':(1,0),'csetm':(0xffffffff,0),
                  'cinc':(7,0xfffffffc),'cneg':(7,5),'cinv':(7,4),
                  'branch':(7,U64-4),'fcsel':(7,U64-4)}[op]
    return truth if yes else false, f

def emission(op,cc):
    if op=='branch': return f'b.{cc} 2f\n    mov x0,x5\n    b 3f\n2:\n    mov x0,x4\n3:'
    if op=='fcsel': return f'fcsel d0,d4,d5,{cc}\n    fmov x0,d0'
    if op=='ccmp': return f'ccmp x0,x1,#5,{cc}\n    mov x0,xzr'
    if op=='ccmn': return f'ccmn w0,#3,#11,{cc}\n    mov x0,xzr'
    if op in ('cset','csetm'): return f'{op} w0,{cc}'
    mnemonic,operands={'csel':('csel','x0,x4,x5'),'cinc':('csinc','w0,w4,w5'),
                       'cneg':('csneg','x0,x4,x5'),'cinv':('csinv','x0,x4,x5')}[op]
    return f'{mnemonic} {operands},{cc}'

def update(h,v): return ((h^v)*0x100000001b3)&U64

asm=['.text']
header=['#include <stdint.h>','typedef void check_fn(uint64_t, uint64_t, uint64_t [2]);']
decl=[]; specs=[]
for src,body in sources.items():
    for cc,name in enumerate(conditions):
        # Full matrix for word/register compares; smaller targeted coverage for others.
        selected=ops if src in ('xr','wr') else (
            ('csel','branch','ccmp') if src in ('gap','replace','entry','shift')
            else ('csel','branch','ccmn','fcsel'))
        for op in selected:
            sym=f'probe_{src}_{name}_{op}'
            h=0xcbf29ce484222325
            for a,b in cases:
                v,nz=result(src,op,cc,a,b);h=update(update(h,v),nz)
            asm += [f'.global {sym}',f'.hidden {sym}',f'.type {sym},%function',sym+':',
                    '    mov x4,#7','    mov x5,#-5']
            if op=='fcsel': asm+=['    fmov d4,x4','    fmov d5,x5']
            asm+=['    '+body,'    '+emission(op,name),'    mrs x3,nzcv','    lsr x3,x3,#28',
                  '    stp x0,x3,[x2]','    ret']
            decl.append(f'extern check_fn {sym};')
            specs.append(f'    {{ {sym}, UINT64_C(0x{h:016x}), "{sym}" }},')
asm+=['.section .note.GNU-stack,"",@progbits']
header+=decl+['static const struct {check_fn *fn; uint64_t digest; const char *name;} specs[] = {']+specs+['};']
header+=['static const uint64_t input[][2] = {']+[f'    {{ UINT64_C(0x{a:x}), UINT64_C(0x{b:x}) }},' for a,b in cases]+['};']
(out/'arm-tcg-conditions.S').write_text('\n'.join(asm)+'\n')
(out/'arm-tcg-conditions.inc').write_text('\n'.join(header)+'\n')
