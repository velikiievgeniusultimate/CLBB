extern uint64_t clbb_rust_pac_ssse3(uint64_t, uint64_t, uint64_t, uint64_t);
static uint64_t rng(uint64_t *s) {
    uint64_t z = (*s += UINT64_C(0x9e3779b97f4a7c15));
    z=(z^(z>>30))*UINT64_C(0xbf58476d1ce4e5b9);
    z=(z^(z>>27))*UINT64_C(0x94d049bb133111eb); return z^(z>>31);
}
static void check(uint64_t a,uint64_t b,uint64_t hi,uint64_t lo) {
    ARMPACKey k={.hi=hi,.lo=lo};
    uint64_t expected=oracle_pauth_computepac(a,b,k);
    if(expected!=production_pauth_computepac(a,b,k) || expected!=reference_pauth_computepac(a,b,k) ||
       expected!=clbb_rust_pac_ssse3(a,b,hi,lo)) {fputs("PAC_MISMATCH\n",stderr);exit(1);}
}
static uint64_t ns(void) {struct timespec t;clock_gettime(CLOCK_THREAD_CPUTIME_ID,&t);return t.tv_sec*UINT64_C(1000000000)+t.tv_nsec;}
static uint64_t bench(int rust,int lanes,unsigned n,uint64_t *sum) {
    uint64_t a[4]={1,2,3,4},start=ns();
    for(unsigned i=0;i<n;i++) for(int l=0;l<lanes;l++) {
        ARMPACKey k={.hi=UINT64_C(0x123456789abcdef0),.lo=UINT64_C(0xfedcba9876543210)};
        a[l]=rust?production_pauth_computepac(a[l],i+l,k):reference_pauth_computepac(a[l],i+l,k);
    }
    uint64_t elapsed=ns()-start;*sum=a[0]^a[1]^a[2]^a[3];return elapsed;
}
int main(void) {
    if(!__builtin_cpu_supports("ssse3")) return 77;
    uint64_t s=0;
    for(unsigned i=0;i<2000000;i++) {uint64_t a=rng(&s),b=rng(&s),hi=rng(&s),lo=rng(&s);check(a,b,hi,lo);}
    uint64_t edges[132]={0,UINT64_MAX,UINT64_C(0xaaaaaaaaaaaaaaaa),UINT64_C(0x5555555555555555)};
    for(unsigned i=0;i<64;i++){edges[4+i]=UINT64_C(1)<<i;edges[68+i]=~(UINT64_C(1)<<i);}
    unsigned e=0;
    for(unsigned i=0;i<132;i++)for(unsigned j=0;j<132;j++) {check(edges[i],edges[j],edges[j],edges[i]);check(edges[i],edges[j],edges[i],edges[j]);e+=2;}
    printf("{\"correct_random\":2000000,\"correct_edges\":%u}\n",e);
    for(int lanes=1;lanes<=4;lanes+=3)for(int repeat=0;repeat<9;repeat++) {
        uint64_t csum=0,rsum=0,c=0,r=0;
        for(int turn=0;turn<2;turn++) {int rust=(repeat+turn)&1;uint64_t sum,t=bench(rust,lanes,400000,&sum);if(rust){r=t;rsum=sum;}else{c=t;csum=sum;}}
        if(csum!=rsum)return 2;
        printf("{\"lanes\":%d,\"repeat\":%d,\"c_ns\":%.6f,\"rust_ns\":%.6f,\"checksum\":\"%016" PRIx64 "\"}\n",lanes,repeat,(double)c/(400000*lanes),(double)r/(400000*lanes),csum);
    }
}
