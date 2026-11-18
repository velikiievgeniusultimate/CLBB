/* Synthetic AArch64 entry points. x0/x1 are operands and x2 is output.
 * The shared tail lives across a direct branch, after all guest flags must
 * have been exported from the translation block. */
.text
.macro begin name
.global \name
.hidden \name
.type \name, %function
\name:
.endm
.macro regcase name, op, reg, a, b
begin \name
    \op \reg, \a, \b
    mov x3, x0
    b .Lread_flags
.endm
regcase add32, adds, w0, w0, w1
regcase sub32, subs, w0, w0, w1
regcase add64, adds, x0, x0, x1
regcase sub64, subs, x0, x0, x1
.macro swapcase name, op, reg, a, b
begin \name
    \op \reg, \a, \b
    mov x3, x1
    b .Lread_flags
.endm
swapcase add32_alias_rhs, adds, w1, w0, w1
swapcase sub64_alias_rhs, subs, x1, x0, x1
.macro cmpcase name, op, a, b
begin \name
    \op \a, \b
    mov x3, xzr
    b .Lread_flags
.endm
cmpcase cmp32, cmp, w0, w1
cmpcase cmp64, cmp, x0, x1
cmpcase cmn32, cmn, w0, w1
cmpcase cmn64, cmn, x0, x1
begin add32_one
    adds w3, w0, #1
    b .Lread_flags
begin sub64_one
    subs x3, x0, #1
    b .Lread_flags
begin sub32_zero
    subs w3, w0, #0
    b .Lread_flags
begin add64_page
    adds x3, x0, #4095, lsl #12
    b .Lread_flags
begin sub64_self
    subs x3, x0, x0
    b .Lread_flags

.macro cond dest, flag, bit
    cset w5, \flag
    orr \dest, \dest, w5, lsl #\bit
.endm
.Lread_flags:
    str x3, [x2]
    mrs x4, nzcv
    lsr x4, x4, #28
    str w4, [x2, #8]
    adc x4, xzr, xzr
    str w4, [x2, #12]
    mov w4, wzr
    cond w4, eq, 0
    cond w4, ne, 1
    cond w4, hs, 2
    cond w4, lo, 3
    cond w4, mi, 4
    cond w4, pl, 5
    cond w4, vs, 6
    cond w4, vc, 7
    cond w4, hi, 8
    cond w4, ls, 9
    cond w4, ge, 10
    cond w4, lt, 11
    cond w4, gt, 12
    cond w4, le, 13
    str w4, [x2, #16]
    b.gt .Lgt
    str wzr, [x2, #20]
    ret
.Lgt:
    mov w4, #1
    str w4, [x2, #20]
    ret
.section .note.GNU-stack,"",@progbits
