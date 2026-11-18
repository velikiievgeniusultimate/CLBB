/* Emulator-owned guest regression, using two private ordinary pages. */
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <signal.h>
#include <setjmp.h>
#include <sys/mman.h>
#include <sys/ucontext.h>
#include <unistd.h>
#ifdef __ANDROID__
#include <jni.h>
#endif

#define SENTINEL_A UINT64_C(0x8877665544332211)
#define SENTINEL_B UINT64_C(0x1020304050607080)
#define INPUT_A UINT64_C(0x93ac1292e76fa845)
#define INPUT_B UINT64_C(0x21eefd4583b4cda8)
#define TOP_TAG UINT64_C(0x5b00000000000000)
#define ARRAYSIZE(x) (sizeof(x)/sizeof((x)[0]))

struct outputs { uint64_t r8, r9, base; };
typedef void (*probe_fn)(void *, struct outputs *);
enum operand { A, B, BASE, ZR };
struct test {
    const char *name;
    probe_fn probe;
    const unsigned char *pc;
    unsigned bytes, store, sign;
    int disp, writeback;
    enum operand first, second;
};
#define DECL(name) extern void name(void *,struct outputs *);extern const unsigned char name##_pc[]
DECL(load_x); DECL(store_x); DECL(load_w); DECL(store_w); DECL(load_sw);
DECL(load_x_pre); DECL(store_x_pre); DECL(load_w_post); DECL(store_w_post); DECL(load_sw_pre); DECL(load_sw_post);
DECL(load_np); DECL(store_np); DECL(load_base_first); DECL(load_base_second); DECL(store_base_first); DECL(store_base_second);
DECL(load_zero_first); DECL(load_zero_second); DECL(store_zero_first); DECL(store_zero_second);
DECL(load_x_offset); DECL(store_x_offset); DECL(load_sp);

#define T(name,width,store,sign,offset,wb,one,two) {#name,name,name##_pc,width,store,sign,offset,wb,one,two}
static const struct test tests[] = {
    T(load_x,8,0,0,0,0,A,B), T(store_x,8,1,0,0,0,A,B), T(load_w,4,0,0,0,0,A,B),
    T(store_w,4,1,0,0,0,A,B), T(load_sw,4,0,1,0,0,A,B),
    T(load_x_pre,8,0,0,16,16,A,B), T(store_x_pre,8,1,0,-16,-16,A,B),
    T(load_w_post,4,0,0,0,8,A,B), T(store_w_post,4,1,0,0,-8,A,B),
    T(load_sw_pre,4,0,1,-4,-4,A,B), T(load_sw_post,4,0,1,0,4,A,B),
    T(load_np,8,0,0,-16,0,A,B), T(store_np,4,1,0,4,0,A,B),
    T(load_base_first,8,0,0,0,0,BASE,B), T(load_base_second,8,0,0,0,0,A,BASE),
    T(store_base_first,8,1,0,0,0,BASE,B), T(store_base_second,8,1,0,0,0,A,BASE),
    T(load_zero_first,8,0,0,0,0,ZR,B), T(load_zero_second,8,0,0,0,0,A,ZR),
    T(store_zero_first,8,1,0,0,0,ZR,B), T(store_zero_second,8,1,0,0,0,A,ZR),
    T(load_x_offset,8,0,0,504,0,A,B), T(store_x_offset,8,1,0,-512,0,A,B),
};

static unsigned checks;
static uint64_t word(const void *p,size_t n) { uint64_t v=0;memcpy(&v,p,n);return v; }
static uint64_t read_register(struct outputs out, enum operand r)
{
    switch (r) { case A: return out.r8;case B: return out.r9;case BASE:return out.base;case ZR:return 0; }
    return 0;
}
static void write_register(struct outputs *out, enum operand r, uint64_t value)
{
    switch(r){case A:out->r8=value;break;case B:out->r9=value;break;case BASE:out->base=value;break;case ZR:break;}
}
static uint64_t extend_word(uint64_t n, const struct test *t)
{
    if (t->bytes==8) return n;
    return t->sign ? (uint64_t)(int64_t)(int32_t)n : (uint32_t)n;
}
static int success(const struct test *t,unsigned char *addr,uint64_t tag)
{
    uint64_t base=(uintptr_t)(addr-t->disp)|tag, ia=INPUT_A, ib=INPUT_B;
    struct outputs expected={SENTINEL_A,SENTINEL_B,base}, actual={0};
    unsigned char ref[24];
    memset(ref,0x5a,sizeof(ref));memcpy(ref+2,&ia,t->bytes);memcpy(ref+2+t->bytes,&ib,t->bytes);
    memcpy(addr-2,ref,sizeof(ref));
    if(t->store) {
        uint64_t a=read_register(expected,t->first), b=read_register(expected,t->second);
        memcpy(ref+2,&a,t->bytes);memcpy(ref+2+t->bytes,&b,t->bytes);
    } else {
        write_register(&expected,t->first,extend_word(ia,t));
        write_register(&expected,t->second,extend_word(ib,t));
    }
    if(t->writeback) expected.base=base+t->writeback;
    t->probe((void *)(uintptr_t)base,&actual);
    ++checks;
    return memcmp(addr-2,ref,sizeof(ref))==0 && !memcmp(&actual,&expected,sizeof(actual));
}

static sigjmp_buf recovery;
static const unsigned char *fault_pc, *fault_addr;
static uintptr_t fault_base;
static volatile sig_atomic_t matched;
static void on_fault(int signo, siginfo_t *info, void *opaque)
{
    ucontext_t *uc=opaque;
    matched=signo==SIGSEGV && (const void *)info->si_addr==fault_addr &&
            uc->uc_mcontext.pc==(uintptr_t)fault_pc &&
            uc->uc_mcontext.regs[0]==fault_base && uc->uc_mcontext.regs[8]==SENTINEL_A &&
            uc->uc_mcontext.regs[9]==SENTINEL_B;
    siglongjmp(recovery,1);
}

static int expect_fault(probe_fn probe)
{
    if (!sigsetjmp(recovery, 1)) {
        struct outputs unused = {0};
        probe((void *)fault_base, &unused);
        return 0;
    }
    return matched;
}

static int guard(const struct test *t,unsigned char *edge,int second,uint64_t tag)
{
    unsigned char *start=second?edge-t->bytes:edge;
    uint64_t base=((uint64_t)(uintptr_t)start-(int64_t)t->disp)|tag;
    const uint64_t mask=t->bytes==8?UINT64_MAX:UINT32_MAX;
    uint64_t expected_first=INPUT_A&mask;
    struct outputs before={SENTINEL_A,SENTINEL_B,base};
    uint64_t input=INPUT_A;
    memcpy(edge-t->bytes,&input,t->bytes);
    if(t->store && second) expected_first=read_register(before,t->first)&mask;
    fault_pc=t->pc;fault_addr=edge;fault_base=base;matched=0;
    if (!expect_fault(t->probe)) return 0;
    ++checks;
    return word(edge-t->bytes,t->bytes)==expected_first;
}
static int contract(void)
{
    int page=getpagesize();
    unsigned char *mem=mmap(NULL,2*(size_t)page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
    if(mem==MAP_FAILED) return __LINE__;
    unsigned char *edge=mem+page;
    struct sigaction sa={0}, old={0};
    sa.sa_sigaction=on_fault;sa.sa_flags=SA_SIGINFO;sigemptyset(&sa.sa_mask);
    if(sigaction(SIGSEGV,&sa,&old) || mprotect(edge,page,PROT_NONE)) return __LINE__;
    for(size_t i=0; i<ARRAYSIZE(tests);++i) {
        const struct test *t=&tests[i];
        for(unsigned tagged=0;tagged<2;tagged++) {
            uint64_t tag=tagged?TOP_TAG:0;
            for(unsigned unaligned=0;unaligned<2;unaligned++) {
                if(!success(t,mem+page/2+unaligned,tag)) {
                    fprintf(stderr,"FAIL normal %s tagged=%u unaligned=%u\n",t->name,tagged,unaligned);return __LINE__;
                }
            }
            for(int second=0;second<=1;second++) {
                if(!guard(t,edge,second,tag)) {
                    fprintf(stderr,"FAIL fault %s tagged=%u second=%d matched=%d\n",t->name,tagged,second,matched);return __LINE__;
                }
            }
        }
    }
    struct test sp=T(load_sp,8,0,0,0,0,A,B);
    if(!success(&sp,mem+page/2,0)) return __LINE__;
    if(sigaction(SIGSEGV,&old,NULL) || munmap(mem,2*(size_t)page)) return __LINE__;
    fprintf(stderr,"CLBB_ARM64_PAIR_CONTRACT_OK=1 cases=%u\n",checks);
    return 0;
}
#ifdef __ANDROID__
JNIEXPORT jint JNICALL Java_CLBBArmLseContract_check(JNIEnv *env,jclass cls)
{
    (void)env;(void)cls;return contract();
}
#else
int main(void) { return contract(); }
#endif
