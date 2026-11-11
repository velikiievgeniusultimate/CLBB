// SPDX-License-Identifier: GPL-3.0-or-later
// Owned differential driver; QEMU implementation and notices stay in its source.
#include <stdint.h>
#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include <dlfcn.h>
#include <time.h>
#include <fenv.h>
typedef struct {int8_t tiny,round;uint8_t flags;int8_t precision;uint8_t ftz,ifz,nan,snan;} Status;
typedef uint32_t (*Op)(uint32_t,uint32_t,uint32_t,int,Status*);
static uint64_t seed=0xa5568f10773cf9b1ULL;
static uint32_t rnd(void){seed^=seed>>12;seed^=seed<<25;seed^=seed>>27;return (seed*0x2545f4914f6cdd1dULL)>>32;}
static double stamp(void){struct timespec t;clock_gettime(CLOCK_PROCESS_CPUTIME_ID,&t);return t.tv_sec+t.tv_nsec/1e9;}
static void verify(Op base,Op variant,uint32_t a,uint32_t b,uint32_t c,int f,Status s,uint64_t at){
 Status t=s;uint32_t x=base(a,b,c,f,&s),y=variant(a,b,c,f,&t);
 if(x!=y||memcmp(&s,&t,sizeof(s))){fprintf(stderr,"MISMATCH at%llu a%08x b%08x c%08x flags%x result%08x/%08x status%02x/%02x\n",(unsigned long long)at,a,b,c,f,x,y,s.flags,t.flags);exit(81);}
}
int main(int argc,char **argv){
 if(argc!=4)return 2;
 void *a=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL),*b=dlopen(argv[2],RTLD_NOW|RTLD_LOCAL);
 if(!a||!b){fprintf(stderr,"dlopen: %s\n",dlerror());return 3;}
 Op base=dlsym(a,"float32_muladd"),variant=dlsym(b,argv[3]);if(!base||!variant)return 4;
 int (*supports)(void)=dlsym(b,"audit_fma_supported");
 if(!strcmp(argv[3],"audit_fma")&&(!supports||!supports()))return 77;
 if(sizeof(Status)!=8)return 5;
 if(fesetround(FE_TONEAREST))return 6;
 uint64_t tested=0;
 static const uint32_t edges[]={0,0x80000000,1,0x80000001,0x007fffff,0x807fffff,0x00800000,0x80800000,0x00800001,0x3f800000,0xbf800000,0x3f800001,0x3f7fffff,0x40000000,0x7f7fffff,0xff7fffff,0x7f800000,0xff800000,0x7fc00000,0xffc00000,0x7f800001,0xff800001};
 for(int round=0;round<6;round++)for(int opts=0;opts<32;opts++)for(int f=0;f<16;f++){
  Status s={.round=round,.tiny=opts&1,.ftz=(opts>>1)&1,.ifz=(opts>>2)&1,.nan=(opts>>3)&1,.snan=0,.flags=(opts&16)?32:0};
  for(unsigned i=0;i<sizeof(edges)/4;i++)for(unsigned j=0;j<sizeof(edges)/4;j++)for(unsigned k=0;k<sizeof(edges)/4;k++)verify(base,variant,edges[i],edges[j],edges[k],f,s,tested++);
 }
 for(int i=0;i<5000000;i++){
  unsigned x=rnd();Status s={.round=x%6,.tiny=(x>>3)&1,.ftz=(x>>4)&1,.ifz=(x>>5)&1,.nan=(x>>6)&1,.flags=(x>>8)&255};
  verify(base,variant,rnd(),rnd(),rnd(),rnd()&15,s,tested++);
 }
 printf("CORRECT %s %llu cases result_and_status\n",argv[3],(unsigned long long)tested);
 uint32_t args[4096][3];
 for(int i=0;i<4096;i++)for(int j=0;j<3;j++){union{float f;uint32_t u;}v={.f=(float)((int)(rnd()%200001)-100000)/127.0f};args[i][j]=v.u;}
 for(int mix=0;mix<2;mix++){
  if(mix)for(int i=0;i<4096;i+=3)args[i][i%2]=0;
  for(int k=0;k<7;k++){
   double result[2];uint32_t sum=0;Op ops[2]={base,variant};
   for(int side=0;side<2;side++){Status s={.flags=32};double at=stamp();for(unsigned n=0;n<8000000;n++){uint32_t *p=args[n&4095];sum^=ops[side](p[0],p[1],p[2],0,&s);}result[side]=(stamp()-at)*1e9/8000000;}
   printf("BENCH mix%d run%d base_ns%.3f variant_ns%.3f ratio%.4f checksum%08x\n",mix,k,result[0],result[1],result[1]/result[0],sum);
  }
 }
}
