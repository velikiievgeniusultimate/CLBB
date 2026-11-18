.text
.p2align 2
.macro init_pair
 movz x8,#0x2211
 movk x8,#0x4433,lsl #16
 movk x8,#0x6655,lsl #32
 movk x8,#0x8877,lsl #48
 movz x9,#0x7080
 movk x9,#0x5060,lsl #16
 movk x9,#0x3040,lsl #32
 movk x9,#0x1020,lsl #48
.endm
.macro begin_pair name
 .global \name, \name\()_pc
 .hidden \name, \name\()_pc
 .type \name,%function
\name:
 init_pair
.endm
.macro finish_pair name
 stp x8,x9,[x1]
 str x0,[x1,#16]
 ret
 .size \name,.-\name
.endm
.macro pair name, op, first, second, addr:vararg
 begin_pair \name
\name\()_pc:
 \op \first,\second,\addr
 finish_pair \name
.endm
pair load_x,ldp,x8,x9,[x0]
pair store_x,stp,x8,x9,[x0]
pair load_w,ldp,w8,w9,[x0]
pair store_w,stp,w8,w9,[x0]
pair load_sw,ldpsw,x8,x9,[x0]
pair load_x_pre,ldp,x8,x9,[x0,#16]!
pair store_x_pre,stp,x8,x9,[x0,#-16]!
pair load_w_post,ldp,w8,w9,[x0],#8
pair store_w_post,stp,w8,w9,[x0],#-8
pair load_sw_pre,ldpsw,x8,x9,[x0,#-4]!
pair load_sw_post,ldpsw,x8,x9,[x0],#4
pair load_np,ldnp,x8,x9,[x0,#-16]
pair store_np,stnp,w8,w9,[x0,#4]
pair load_base_first,ldp,x0,x9,[x0]
pair load_base_second,ldp,x8,x0,[x0]
pair store_base_first,stp,x0,x9,[x0]
pair store_base_second,stp,x8,x0,[x0]
pair load_zero_first,ldp,xzr,x9,[x0]
pair load_zero_second,ldp,x8,xzr,[x0]
pair store_zero_first,stp,xzr,x9,[x0]
pair store_zero_second,stp,x8,xzr,[x0]
pair load_x_offset,ldp,x8,x9,[x0,#504]
pair store_x_offset,stp,x8,x9,[x0,#-512]
begin_pair load_sp
 mov x12,sp
 mov sp,x0
load_sp_pc:
 ldp x8,x9,[sp]
 mov sp,x12
 finish_pair load_sp
.section .note.GNU-stack,"",%progbits
