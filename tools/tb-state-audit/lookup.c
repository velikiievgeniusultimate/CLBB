#include "qemu/osdep.h"
#include "cpu.h"
#include "exec/exec-all.h"
#include "exec/tb-hash.h"
#include <dlfcn.h>
#include <time.h>
#include <stdio.h>
static uint64_t nano(void){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return(uint64_t)t.tv_sec*1000000000+t.tv_nsec;}
int main(int argc,char **argv){
 if(argc!=3)return 2;
 ARMCPU *ac=calloc(1,sizeof(*ac));CPUState*c=&ac->parent_obj;CPUARMState*e=&ac->env;c->env_ptr=e;e->hflags=R_TBFLAG_ANY_AARCH64_STATE_MASK;
 TranslationBlock *tb=calloc(64,sizeof(*tb));
 for(unsigned i=0;i<64;i++){tb[i].pc=0x400000+i*4;tb[i].flags=e->hflags;tb[i].tc.ptr=(void*)(uintptr_t)(0x800000+i*32);c->tb_jmp_cache[tb_jmp_cache_hash_func(tb[i].pc)]=&tb[i];}
 const unsigned n=12000000;uint64_t expected=0;
 for(int which=1;which<=2;which++){
  void*h=dlopen(argv[which],RTLD_NOW|RTLD_LOCAL);if(!h){puts(dlerror());return 2;}
  void*(*lookup)(CPUARMState*)=(void*(*)(CPUARMState*))dlsym(h,"helper_lookup_tb_ptr");assert(lookup);
  for(unsigned trial=0;trial<5;trial++){
   uint64_t sum=0,start=nano();
   for(unsigned i=0;i<n;i++){unsigned j=i&63;e->pc=tb[j].pc;void*out=lookup(e);assert(out==tb[j].tc.ptr);sum+=(uintptr_t)out;}
   uint64_t took=nano()-start;if(!expected)expected=sum;assert(sum==expected);
   printf("TB_LOOKUP_BENCH variant=%d trial=%u ns=%.4f checksum=%llu\n",which,trial,(double)took/n,(unsigned long long)sum);
  }
 }
 free(tb);free(ac);return 0;
}
