#!/usr/bin/env python3
"""Emit independent integer reference values for the synthetic guest probes."""
from itertools import product
from pathlib import Path
import sys

out = Path(sys.argv[1])
edges = [0,1,2,127,0x7fffffff,0x80000000,0xfffffffe,0xffffffff,
         1<<32,(1<<32)+1,(1<<63)-1,1<<63,(1<<64)-1,0x9999999999999999]
# Order agrees with the table of functions in contract.c.
ops = [(32,False),(32,True),(64,False),(64,True),(32,False),(64,True),
       (32,True),(64,True),(32,False),(64,False),
       (32,False),(64,True),(32,True),(64,False),(64,True)]
with out.open('w') as f:
    f.write('static const struct vector vectors[] = {\n')
    for idx, (width, sub) in enumerate(ops):
        mod = 1 << width
        sign = mod >> 1
        for a, b in product(edges, edges if idx < 10 else [0]):
            rhs = b if idx < 10 else [1,1,0,4095<<12,a][idx-10]
            x, y = a % mod, rhs % mod
            raw = x-y if sub else x+y
            result = raw % mod
            sx = x-mod if x&sign else x
            sy = y-mod if y&sign else y
            signed = sx-sy if sub else sx+sy
            n, z, c, v = bool(result&sign), result==0, x>=y if sub else raw>=mod, not -sign<=signed<sign
            cond = (z,not z,c,not c,n,not n,v,not v,c and not z,(not c) or z,
                    n==v,n!=v,(not z) and n==v,z or n!=v)
            nzcv = (n<<3)|(z<<2)|(c<<1)|v
            mask = sum(int(flag)<<i for i,flag in enumerate(cond))
            expected = 0 if idx in (6,7,8,9) else result
            f.write(f'{{UINT64_C(0x{a:x}),UINT64_C(0x{b:x}),UINT64_C(0x{expected:x}),{nzcv},{mask},{idx}}},\n')
    f.write('};\n')
