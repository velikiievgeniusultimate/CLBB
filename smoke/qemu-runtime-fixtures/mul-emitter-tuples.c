/* SPDX-License-Identifier: GPL-2.0-or-later
 * Execute only owned instructions emitted by exact public TCG scalar MUL.
 * Local function address comes from pinned public ELF symbol, never guest/JIT. */
#include "qemu/osdep.h"
#include "qemu.h"
#include "cpu.h"
#include "tcg/tcg.h"
#include "tcg/tcg-op.h"
#include <dlfcn.h>
#include <sys/mman.h>
static unsigned checks,cases,blocks,branches[5];
#define CHECK(c) do { checks++;if(!(c)){fprintf(stderr,"FAIL line%d checks%u: %s\n",__LINE__,checks,#c);exit(1);} }while(0)
static uint64_t seed=UINT64_C(0xa97d103b5ec24f68);
static uint64_t rnd(void){seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;return seed;}
static void byte(TCGContext*s,unsigned b){*s->code_ptr++=b;}
/* MOV r64(dest),r64(src), independent owned ABI setup/return. */
static void mov64(TCGContext*s,unsigned dest,unsigned src){
 byte(s,0x48|(dest>=8?4:0)|(src>=8?1:0));byte(s,0x8b);byte(s,0xc0|((dest&7)<<3)|(src&7));
}
typedef void (*emitter_fn)(TCGContext*,TCGOpcode,const TCGArg*,const int*);
typedef uint64_t (*native_fn)(uint64_t,uint64_t,uint64_t*);
static uint64_t narrow(unsigned w,uint64_t a){return w==32?(uint32_t)a:a;}
static unsigned overflow(unsigned w,uint64_t x,uint64_t y){
 if(w==32){int64_t z=(int64_t)(int32_t)x*(int64_t)(int32_t)y;return z<INT32_MIN||z>INT32_MAX;}
 __int128 z=(__int128)(int64_t)x*(__int128)(int64_t)y;return z<INT64_MIN||z>INT64_MAX;
}
static void exercise(native_fn code,unsigned w,unsigned pattern,bool isconst,uint64_t k,uint64_t a,uint64_t b){
 uint64_t x=narrow(w,a),y=narrow(w,isconst?k:pattern>=3?a:b),flags=0;
 uint64_t result=code(a,b,&flags),expected=narrow(w,x*y);CHECK(result==expected);
 unsigned of=overflow(w,x,y);CHECK((flags&1)==of);CHECK(!!(flags&0x800)==of);cases++;
}
static void block(emitter_fn emit,unsigned w,unsigned group,unsigned pattern,bool isconst,uint64_t k){
 /* Patterns exhaust: distinct, out=first, out=second, inputs=same,
  * all=same. Constant RHS uses only distinct/out=first. */
 unsigned first=group?8:0,second=group?9:1,other=group?10:2;
 unsigned a1=first,a2=pattern>=3?first:second;
 unsigned a0=pattern==1||pattern==4?first:pattern==2?second:other;
 void*memory=mmap(NULL,4096,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);CHECK(memory!=MAP_FAILED);
 TCGContext s={0};s.code_buf=s.code_ptr=memory;
 mov64(&s,11,2); /* save flags pointer before possible output RDX */
 mov64(&s,a1,7);if(!isconst&&a2!=a1)mov64(&s,a2,6);
 size_t begin=(uint8_t*)s.code_ptr-(uint8_t*)memory;
 TCGArg args[]={a0,a1,isconst?k:a2};int constants[]={0,0,isconst};
 emit(&s,w==32?INDEX_op_mul_i32:INDEX_op_mul_i64,args,constants);
 size_t end=(uint8_t*)s.code_ptr-(uint8_t*)memory;CHECK(end>begin&&end-begin<32);
 /* Capture defined CF/OF before any later instruction changes flags. */
 byte(&s,0x9c);byte(&s,0x5f); /* pushfq; pop rdi */
 byte(&s,0x49);byte(&s,0x89);byte(&s,0x3b); /* mov [r11],rdi */
 mov64(&s,0,a0);byte(&s,0xc3);CHECK((uint8_t*)s.code_ptr-(uint8_t*)memory<128);
 CHECK(mprotect(memory,4096,PROT_READ|PROT_EXEC)==0);
 const uint64_t edge[]={0,1,2,3,UINT64_MAX,UINT64_MAX-1,INT32_MAX,UINT64_C(0x80000000),UINT64_C(0xffffffff),INT64_MAX,UINT64_C(0x8000000000000000),UINT64_C(0x123456789abcdef0)};
 for(unsigned i=0;i<ARRAY_SIZE(edge);i++)for(unsigned j=0;j<ARRAY_SIZE(edge);j++)exercise(memory,w,pattern,isconst,k,edge[i],edge[j]);
 for(unsigned i=0;i<256;i++)exercise(memory,w,pattern,isconst,k,rnd(),rnd());
 CHECK(munmap(memory,4096)==0);blocks++;if(!isconst)branches[pattern]++;
}
int main(int argc,char**argv){
 alarm(15);CHECK(argc==4);unsigned fixed=strtoul(argv[3],NULL,0);CHECK(fixed<=1);
 void*h=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);CHECK(h);Dl_info di;CHECK(dladdr((void*)tcg_func_start,&di));
 char actual[PATH_MAX],wanted[PATH_MAX];CHECK(realpath(di.dli_fname,actual)&&realpath(argv[1],wanted)&&!strcmp(actual,wanted));
 uintptr_t offset=strtoull(argv[2],NULL,0);CHECK(offset&&offset<UINT64_C(0x10000000));emitter_fn emit=(emitter_fn)((uintptr_t)di.dli_fbase+offset);
 Dl_info target;CHECK(dladdr((void*)emit,&target)&&target.dli_fbase==di.dli_fbase);
 uint64_t ks[]={0,1,UINT64_MAX,3,7,(uint64_t)-3,127,128,(uint64_t)-128,(uint64_t)-129,INT32_MAX,(uint64_t)(int64_t)INT32_MIN,UINT64_C(0x80000000),UINT64_C(0xffffffff)};
 for(unsigned w=32;w<=64;w+=32)for(unsigned group=0;group<2;group++){
  for(unsigned p=0;p<5;p++)if(fixed||p==1||p==4)block(emit,w,group,p,false,0);
  for(unsigned p=0;p<2;p++)if(fixed||p==1)for(unsigned k=0;k<ARRAY_SIZE(ks);k++){
   /* Match unchanged e constraint: arbitrary i32 / signed32-only i64. */
   if(w==64&&ks[k]!=(uint64_t)(int64_t)(int32_t)ks[k])continue;
   block(emit,w,group,p,true,ks[k]);
  }
 }
 if(fixed)for(unsigned p=0;p<5;p++)CHECK(branches[p]==4);
 printf("{\"pass\":true,\"fixed\":%u,\"blocks\":%u,\"cases\":%u,\"checks\":%u,\"dynamic_physical_partition_blocks\":[%u,%u,%u,%u,%u],\"CF_OF_checked\":true,\"owned_public_emitter_only\":true,\"diagnostic_hooks\":false,\"game\":false,\"benchmark\":false}\n",fixed,blocks,cases,checks,branches[0],branches[1],branches[2],branches[3],branches[4]);return 0;
}
