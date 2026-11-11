/* SPDX-License-Identifier: GPL-2.0-or-later
 * Owned public TCG multiplication IR; no guest code, ART, game or services.
 * Setup API follows the existing owned lzcnt-ir fixture. */
#include "qemu/osdep.h"
#include "qemu.h"
#include "cpu.h"
#include "exec/exec-all.h"
#include "sysemu/tcg.h"
#include "sysemu/cpus.h"
#include "qemu/module.h"
#include "tcg/tcg.h"
#include "tcg/tcg-op.h"
#include <dlfcn.h>
#define CHECK(c) do { checks++; if (!(c)) { fprintf(stderr,"FAIL line%d checks%u: %s\n",__LINE__,checks,#c); exit(1); } } while(0)
static unsigned checks, cases, blocks, mul_before, mul_after, bytes;
static unsigned retained_immediate, retained_dynamic;
static CPUState *cs;
static CPUARMState *env;
static uint64_t seed = UINT64_C(0x17dabe6421c89f03);
static uint64_t rnd(void) { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; return seed; }
static uint64_t narrow(unsigned w, uint64_t x) { return w==32 ? (uint32_t)x : x; }
static unsigned count_mul(unsigned w) {
    unsigned count=0; TCGOp *op;
    QTAILQ_FOREACH(op,&tcg_ctx->ops,link) if(op->opc==(w==32?INDEX_op_mul_i32:INDEX_op_mul_i64)) count++;
    return count;
}
static void store32(TCGv_i32 value, TCGv_ptr base, tcg_target_long offset) {
    TCGv_i64 wide=tcg_temp_new_i64();
    tcg_gen_extu_i32_i64(wide,value);tcg_gen_st_i64(wide,base,offset);tcg_temp_free_i64(wide);
}
/* alias: 0=distinct,1=out first,2=out second,3=identical inputs,
 * 4=identical inputs and output. constant variants use aliases 0..2.
 * live mask controls post-multiply uses; pressure adds live runtime temps. */
#define GENERATE_BODY(W,T,NEW,LD,MOVI,MUL,ST) do { \
    T x=NEW(), z=alias>=3?x:NEW(), y=alias==1||alias==4?x:alias==2?z:NEW(); \
    T p[8]; \
    LD(x,cpu_env,offsetof(CPUARMState,xregs[0])); \
    if(alias<3) { if(isconst) MOVI(z,k); else LD(z,cpu_env,offsetof(CPUARMState,xregs[1])); } \
    if(pressure) for(unsigned j=0;j<8;j++) { p[j]=NEW(); LD(p[j],cpu_env,offsetof(CPUARMState,xregs[8])+8*j); } \
    MUL(y,x,z); \
    ST(y,cpu_env,offsetof(CPUARMState,xregs[2])); \
    if(live&1) ST(x,cpu_env,offsetof(CPUARMState,xregs[3])); \
    if(live&2) ST(z,cpu_env,offsetof(CPUARMState,xregs[4])); \
    if(pressure) for(unsigned j=0;j<8;j++) ST(p[j],cpu_env,offsetof(CPUARMState,xregs[16])+8*j); \
} while(0)
static void *generate(unsigned w,unsigned alias,unsigned live,bool isconst,uint64_t k,bool pressure) {
    TranslationBlock *tb=tcg_tb_alloc(tcg_ctx); CHECK(tb);
    tb->tc.ptr=tcg_ctx->code_gen_ptr;tb->pc=0;tb->cs_base=0;tb->flags=0;tb->cflags=0;tb->icount=1;tcg_ctx->tb_cflags=0;
    tcg_func_start(tcg_ctx);tcg_gen_insn_start(0,0,0);
    if(w==32) GENERATE_BODY(32,TCGv_i32,tcg_temp_new_i32,tcg_gen_ld_i32,tcg_gen_movi_i32,tcg_gen_mul_i32,store32);
    else GENERATE_BODY(64,TCGv_i64,tcg_temp_new_i64,tcg_gen_ld_i64,tcg_gen_movi_i64,tcg_gen_mul_i64,tcg_gen_st_i64);
    tcg_gen_exit_tb(NULL,0);
    tcg_ctx->tb_jmp_reset_offset=tb->jmp_reset_offset;tcg_ctx->tb_jmp_insn_offset=tb->jmp_target_arg;tcg_ctx->tb_jmp_target_addr=NULL;
    CHECK(count_mul(w)==1);mul_before++;
    int size=tcg_gen_code(tcg_ctx,tb);CHECK(size>0&&size<8192);
    unsigned after=count_mul(w);CHECK(after<=1);
    /* Nontrivial constants exercise backend MUL, not frontend identities. */
    if(!isconst || (narrow(w,k)!=0 && narrow(w,k)!=1 && narrow(w,k)!=narrow(w,UINT64_MAX))) CHECK(after==1);
    mul_after+=after;if(after){if(isconst)retained_immediate++;else retained_dynamic++;}
    bytes+=size;blocks++;tcg_ctx->code_gen_ptr=(void*)QEMU_ALIGN_UP((uintptr_t)tb->tc.ptr+size,CODE_GEN_ALIGN);
    return tb->tc.ptr;
}
static void exercise(void *code,unsigned w,unsigned alias,unsigned live,bool isconst,uint64_t k,bool pressure,uint64_t a,uint64_t b) {
    uint64_t x=narrow(w,a), z=narrow(w,alias>=3?a:isconst?k:b), product=narrow(w,x*z);
    uint64_t postx=alias==1||alias==4?product:x;
    uint64_t postz=alias==2||alias==4?product:z;
    env->xregs[0]=a;env->xregs[1]=b;
    /* i32 values are explicitly widened and stored as 64 bits. */
    env->xregs[2]=env->xregs[3]=env->xregs[4]=0;
    for(unsigned j=0;j<8;j++){env->xregs[8+j]=rnd();env->xregs[16+j]=0;}
    CHECK(tcg_qemu_tb_exec(env,code)==0);
    CHECK(env->xregs[2]==product);
    if(live&1)CHECK(env->xregs[3]==postx);
    if(live&2)CHECK(env->xregs[4]==postz);
    CHECK(env->xregs[0]==a&&env->xregs[1]==b);
    if(pressure)for(unsigned j=0;j<8;j++)CHECK(env->xregs[16+j]==narrow(w,env->xregs[8+j]));
    cases++;
}
int main(int argc,char **argv) {
    alarm(30);CHECK(argc==2);
    void *h=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);CHECK(h);
    Dl_info di;CHECK(dladdr((void*)tcg_func_start,&di));char actual[PATH_MAX],wanted[PATH_MAX];
    CHECK(realpath(di.dli_fname,actual)&&realpath(argv[1],wanted)&&!strcmp(actual,wanted));
    qemu_init_cpu_list();tcg_exec_init(32ul*1024*1024);module_call_init(MODULE_INIT_QOM);
    cs=cpu_create(ARM_CPU_TYPE_NAME("max"));cpu_reset(cs);thread_cpu=cs;
    tcg_prologue_init(tcg_ctx);tcg_region_init();env=cs->env_ptr;
    const uint64_t constants[]={0,1,UINT64_MAX,3,7,(uint64_t)-3,127,128,(uint64_t)-128,(uint64_t)-129,INT32_MAX,(uint64_t)(int64_t)INT32_MIN,UINT64_C(0x80000000),UINT64_C(0xffffffff),UINT64_C(0x100000003),UINT64_C(0x8000000000000001)};
    const uint64_t edges[]={0,1,2,3,UINT64_MAX,UINT64_MAX-1,INT32_MAX,UINT64_C(0x80000000),UINT64_C(0xffffffff),UINT64_C(0x100000000),INT64_MAX,UINT64_C(0x8000000000000000),UINT64_C(0x123456789abcdef0)};
    for(unsigned w=32;w<=64;w+=32)for(unsigned isconst=0;isconst<=1;isconst++)
    for(unsigned ki=0;ki<(isconst?ARRAY_SIZE(constants):1);ki++)
    for(unsigned alias=0;alias<(isconst?3:5);alias++)for(unsigned live=0;live<4;live++)for(unsigned pressure=0;pressure<=1;pressure++){
        uint64_t k=constants[ki];void *code=generate(w,alias,live,isconst,k,pressure);
        for(unsigned i=0;i<ARRAY_SIZE(edges);i++)for(unsigned j=0;j<ARRAY_SIZE(edges);j++)exercise(code,w,alias,live,isconst,k,pressure,edges[i],edges[j]);
        for(unsigned i=0;i<64;i++)exercise(code,w,alias,live,isconst,k,pressure,rnd(),rnd());
    }
    CHECK(mul_before==blocks&&retained_dynamic&&retained_immediate);
    printf("{\"pass\":true,\"blocks\":%u,\"cases\":%u,\"checks\":%u,\"mul_before\":%u,\"mul_after\":%u,\"retained_constant\":%u,\"retained_dynamic\":%u,\"generated_bytes_descriptive\":%u,\"game\":false,\"benchmark\":false}\n",blocks,cases,checks,mul_before,mul_after,retained_immediate,retained_dynamic,bytes);
    return 0;
}
