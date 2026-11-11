/* Production ARM header fast path against the unmodified complete state getter
 * in a real bridge library. No VM, game, input or process memory is created. */
#include "qemu/osdep.h"
#include "cpu.h"
#include <dlfcn.h>
#include <time.h>
#include <stdio.h>
typedef void (*Getter)(CPUARMState *,target_ulong *,target_ulong *,uint32_t *);
static Getter original;
static unsigned long fallback_calls;
void arm_get_tb_cpu_state(CPUARMState *env,target_ulong *pc,target_ulong *cs,uint32_t *flags) {
    fallback_calls++;
    original(env,pc,cs,flags);
}
static uint64_t seed=0x104dd33a598678f1ULL;
static uint64_t random64(void) {seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;return seed;}
static uint64_t nano(void) {struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return (uint64_t)t.tv_sec*1000000000+t.tv_nsec;}
static void expect(CPUARMState *env) {
    target_ulong a,b,c,d;uint32_t x,y;
    original(env,&a,&b,&x);cpu_get_tb_cpu_state(env,&c,&d,&y);
    if(a!=c||b!=d||x!=y){fprintf(stderr,"TB_STATE_MISMATCH hflags=%x pc=%llx flags=%x/%x\n",env->hflags,(unsigned long long)env->pc,x,y);abort();}
}
int main(int argc,char **argv) {
    if(argc!=2)return 2;
    void *h=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);
    if(!h){fprintf(stderr,"%s\n",dlerror());return 2;}
    original=(Getter)dlsym(h,"arm_get_tb_cpu_state");
    if(!original)original=(Getter)dlsym(h,"cpu_get_tb_cpu_state");
    assert(original);
    ARMCPU *cpu=calloc(1,sizeof(*cpu));assert(cpu);CPUARMState *env=&cpu->env;
    unsigned cases=0;
    for(unsigned i=0;i<1000000;i++) {
        env->pc=random64();env->hflags=(uint32_t)random64();env->pstate=(uint32_t)random64();env->btype=(uint32_t)random64();
        cpu->isar.id_aa64pfr1=random64();env->regs[15]=(uint32_t)random64();env->thumb=i&1;env->condexec_bits=(i>>1)&255;
        env->vfp.vec_len=(i>>9)&7;env->vfp.vec_stride=(i>>12)&3;env->vfp.xregs[ARM_VFP_FPEXC]=(i&4)?1U<<30:0;
        expect(env);cases++;
    }
    for(unsigned bt=0;bt<16;bt++)for(unsigned btype=0;btype<4;btype++)for(unsigned ss=0;ss<8;ss++) {
        cpu->isar.id_aa64pfr1=bt;env->hflags=R_TBFLAG_ANY_AARCH64_STATE_MASK;
        if(ss&1)env->hflags|=R_TBFLAG_ANY_SS_ACTIVE_MASK;
        if(ss&2)env->hflags|=R_TBFLAG_ANY_PSTATE_SS_MASK;
        env->pstate=(ss&4)?PSTATE_SS:0;env->btype=btype;expect(env);cases++;
    }
    unsigned long slow_before=fallback_calls;
    const unsigned n=4000000;uint64_t checks[2]={0},times[2];
    for(unsigned mode=0;mode<2;mode++) {
        env->hflags=R_TBFLAG_ANY_AARCH64_STATE_MASK|R_TBFLAG_ANY_SS_ACTIVE_MASK;cpu->isar.id_aa64pfr1=1;
        uint64_t start=nano();
        for(unsigned i=0;i<n;i++) {
            target_ulong pc,cs;uint32_t flags;env->pc=(uint64_t)i*4+0x400000;env->pstate=(i&1)?PSTATE_SS:0;env->btype=i&3;
            if(mode)cpu_get_tb_cpu_state(env,&pc,&cs,&flags);else original(env,&pc,&cs,&flags);
            checks[mode]+=pc^cs^flags;
        }
        times[mode]=nano()-start;
    }
    assert(checks[0]==checks[1]);assert(fallback_calls==slow_before);
    printf("TB_STATE_OK cases=%u a32_fallbacks=%lu original_ns=%.3f inline_ns=%.3f speedup=%.2f checksum=%llu\n",cases,fallback_calls,(double)times[0]/n,(double)times[1]/n,(double)times[0]/times[1],(unsigned long long)checks[0]);
    free(cpu);dlclose(h);return 0;
}
