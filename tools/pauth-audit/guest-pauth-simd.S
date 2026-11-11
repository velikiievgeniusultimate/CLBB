.text
.arch armv8.3-a+simd
.global clbb_pauth_simd_stress
.type clbb_pauth_simd_stress, %function
clbb_pauth_simd_stress:
    sub sp, sp, #128
    stp q8, q9, [sp, #0]
    stp q10, q11, [sp, #32]
    stp q12, q13, [sp, #64]
    stp q14, q15, [sp, #96]
    ldp q0, q1, [x0, #0]
    ldp q2, q3, [x0, #32]
    ldp q4, q5, [x0, #64]
    ldp q6, q7, [x0, #96]
    ldp q8, q9, [x0, #128]
    ldp q10, q11, [x0, #160]
    ldp q12, q13, [x0, #192]
    ldp q14, q15, [x0, #224]
    ldp q16, q17, [x0, #256]
    ldp q18, q19, [x0, #288]
    ldp q20, q21, [x0, #320]
    ldp q22, q23, [x0, #352]
    ldp q24, q25, [x0, #384]
    ldp q26, q27, [x0, #416]
    ldp q28, q29, [x0, #448]
    ldp q30, q31, [x0, #480]
    mov w13, #0
    movz x11, #0x6789
    movk x11, #0x2345, lsl #16
.Lstress_loop:
    mov x10, #0x1357
    orr x10, x10, x2
    eor v0.16b, v0.16b, v31.16b
    eor v1.16b, v1.16b, v31.16b
    eor v2.16b, v2.16b, v31.16b
    eor v3.16b, v3.16b, v31.16b
    eor v4.16b, v4.16b, v31.16b
    eor v5.16b, v5.16b, v31.16b
    eor v6.16b, v6.16b, v31.16b
    eor v7.16b, v7.16b, v31.16b
    eor v8.16b, v8.16b, v31.16b
    eor v9.16b, v9.16b, v31.16b
    eor v10.16b, v10.16b, v31.16b
    eor v11.16b, v11.16b, v31.16b
    eor v12.16b, v12.16b, v31.16b
    eor v13.16b, v13.16b, v31.16b
    eor v14.16b, v14.16b, v31.16b
    eor v15.16b, v15.16b, v31.16b
    eor v16.16b, v16.16b, v31.16b
    eor v17.16b, v17.16b, v31.16b
    eor v18.16b, v18.16b, v31.16b
    eor v19.16b, v19.16b, v31.16b
    eor v20.16b, v20.16b, v31.16b
    eor v21.16b, v21.16b, v31.16b
    eor v22.16b, v22.16b, v31.16b
    eor v23.16b, v23.16b, v31.16b
    eor v24.16b, v24.16b, v31.16b
    eor v25.16b, v25.16b, v31.16b
    eor v26.16b, v26.16b, v31.16b
    eor v27.16b, v27.16b, v31.16b
    eor v28.16b, v28.16b, v31.16b
    eor v29.16b, v29.16b, v31.16b
    eor v30.16b, v30.16b, v31.16b
    mov x9, x11
    pacia x9, x10
    cmp x9, x11
    cset w12, ne
    orr w13, w13, w12
    autia x9, x10
    cmp x9, x11
    b.ne .Lauth_failure
    mov x9, x11
    pacib x9, x10
    cmp x9, x11
    cset w12, ne
    orr w13, w13, w12
    autib x9, x10
    cmp x9, x11
    b.ne .Lauth_failure
    mov x9, x11
    pacda x9, x10
    cmp x9, x11
    cset w12, ne
    orr w13, w13, w12
    autda x9, x10
    cmp x9, x11
    b.ne .Lauth_failure
    mov x9, x11
    pacdb x9, x10
    cmp x9, x11
    cset w12, ne
    orr w13, w13, w12
    autdb x9, x10
    cmp x9, x11
    b.ne .Lauth_failure
    pacga x14, x11, x10
    pacga x15, x11, x10
    cmp x14, x15
    b.ne .Lga_failure
    mov x9, x11
    pacia x9, x10
    eor x9, x9, #0x0040000000000000
    autia x9, x10
    cmp x9, x11
    b.eq .Lcorrupt_failure
    eor v0.16b, v0.16b, v31.16b
    eor v1.16b, v1.16b, v31.16b
    eor v2.16b, v2.16b, v31.16b
    eor v3.16b, v3.16b, v31.16b
    eor v4.16b, v4.16b, v31.16b
    eor v5.16b, v5.16b, v31.16b
    eor v6.16b, v6.16b, v31.16b
    eor v7.16b, v7.16b, v31.16b
    eor v8.16b, v8.16b, v31.16b
    eor v9.16b, v9.16b, v31.16b
    eor v10.16b, v10.16b, v31.16b
    eor v11.16b, v11.16b, v31.16b
    eor v12.16b, v12.16b, v31.16b
    eor v13.16b, v13.16b, v31.16b
    eor v14.16b, v14.16b, v31.16b
    eor v15.16b, v15.16b, v31.16b
    eor v16.16b, v16.16b, v31.16b
    eor v17.16b, v17.16b, v31.16b
    eor v18.16b, v18.16b, v31.16b
    eor v19.16b, v19.16b, v31.16b
    eor v20.16b, v20.16b, v31.16b
    eor v21.16b, v21.16b, v31.16b
    eor v22.16b, v22.16b, v31.16b
    eor v23.16b, v23.16b, v31.16b
    eor v24.16b, v24.16b, v31.16b
    eor v25.16b, v25.16b, v31.16b
    eor v26.16b, v26.16b, v31.16b
    eor v27.16b, v27.16b, v31.16b
    eor v28.16b, v28.16b, v31.16b
    eor v29.16b, v29.16b, v31.16b
    eor v30.16b, v30.16b, v31.16b
    subs x2, x2, #1
    b.ne .Lstress_loop
    mov w0, w13
    b .Lfinish
.Lauth_failure:
    mov w0, #2
    b .Lfinish
.Lga_failure:
    mov w0, #3
    b .Lfinish
.Lcorrupt_failure:
    mov w0, #4
.Lfinish:
    stp q0, q1, [x1, #0]
    stp q2, q3, [x1, #32]
    stp q4, q5, [x1, #64]
    stp q6, q7, [x1, #96]
    stp q8, q9, [x1, #128]
    stp q10, q11, [x1, #160]
    stp q12, q13, [x1, #192]
    stp q14, q15, [x1, #224]
    stp q16, q17, [x1, #256]
    stp q18, q19, [x1, #288]
    stp q20, q21, [x1, #320]
    stp q22, q23, [x1, #352]
    stp q24, q25, [x1, #384]
    stp q26, q27, [x1, #416]
    stp q28, q29, [x1, #448]
    stp q30, q31, [x1, #480]
    ldp q8, q9, [sp, #0]
    ldp q10, q11, [sp, #32]
    ldp q12, q13, [sp, #64]
    ldp q14, q15, [sp, #96]
    add sp, sp, #128
    ret
.size clbb_pauth_simd_stress, .-clbb_pauth_simd_stress
.section .note.GNU-stack,"",%progbits
