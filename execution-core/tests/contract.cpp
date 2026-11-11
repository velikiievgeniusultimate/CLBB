/* SPDX-License-Identifier: GPL-2.0-or-later */
#include "GuestCpu.h"
#include "GuestCore.h"
#include <atomic>
#include <cassert>
#include <cstring>
#include <thread>
#include <vector>
#include <cstdio>

static std::atomic<unsigned> acquired{0}, released{0}, calls{0};
static thread_local bool borrowed = false;
static thread_local uint64_t borrowed_cpu = 77;
static int32_t init(const char *a, const char *b) {
    assert(!strcmp(a,"contract") && !strcmp(b,"temp")); return -17;
}
static intptr_t lookup(const char *s) { return !strcmp(s,"known") ? 0x1234 : 0; }
static void *acquire(uint32_t *owned) {
    if (borrowed) { *owned=0; return &borrowed_cpu; }
    *owned=1; ++acquired; return new uint64_t(99);
}
static void release(void *cpu) { assert(*static_cast<uint64_t *>(cpu)==99);++released;delete static_cast<uint64_t *>(cpu); }
static intptr_t word(void *cpu, const ClbbExecutionCallV1 *c) {
    assert(cpu); ++calls;
    if(c->address==1) return -1234567;
    if(c->address==2) return GuestCpu::get()->call(1); // Re-enter same TLS CPU.
    if(c->address==3) {
        for(int i=0;i<8;i++)assert(c->args[i]==i+11);
        assert(c->stack_size==9 && c->stack[0]==0xab && c->stack[8]==0xcd);
        return -7;
    }
    return 0;
}
static uint64_t wide(void *cpu,const ClbbExecutionCallV1 *c) {
    assert(cpu && c->address==4);return UINT64_C(0xfedcba9876543210);
}
const ClbbExecutionBackendV1 *clbb_default_execution_backend_v1() {
    static const ClbbExecutionBackendV1 b={1,sizeof(ClbbExecutionBackendV1),init,lookup,acquire,release,word,wide};return &b;
}
static void *no_cpu(uint32_t *) {return nullptr;}
static void *bad_ownership(uint32_t *owned) {*owned=2;return &borrowed_cpu;}
int main() {
    const auto *b=clbb_default_execution_backend_v1();
    assert(!GuestCore::initialize("contract","temp"));
    assert(GuestCore::lookup_symbol("known")==0x1234 && GuestCore::lookup_symbol("missing")==0);
    intptr_t result=999;int32_t init_result=0;
    assert(clbb_execution_initialize_v1(b,"contract","temp",&init_result)==0 && init_result==-17);
    ClbbExecutionCallV1 c={1,{},nullptr,0};uint64_t fake=1;
    auto bad=*b;bad.abi=2;
    assert(clbb_execution_call_word_v1(&bad,&fake,&c,&result)==-1 && result==999);
    bad=*b;bad.size--;
    assert(clbb_execution_call_word_v1(&bad,&fake,&c,&result)==-1);
    bad=*b;bad.call_u64=nullptr;
    assert(clbb_execution_call_word_v1(&bad,&fake,&c,&result)==-1);
    assert(clbb_execution_call_word_v1(nullptr,&fake,&c,&result)==-2);
    assert(clbb_execution_call_word_v1(b,nullptr,&c,&result)==-2);
    assert(clbb_execution_call_word_v1(b,&fake,nullptr,&result)==-2);
    assert(clbb_execution_call_word_v1(b,&fake,&c,nullptr)==-2);
    c.address=0;assert(clbb_execution_call_word_v1(b,&fake,&c,&result)==-2);
    c.address=1;c.stack_size=-1;assert(clbb_execution_call_word_v1(b,&fake,&c,&result)==-2);
    c.stack_size=1;assert(clbb_execution_call_word_v1(b,&fake,&c,&result)==-2);
    assert(calls==0 && result==999);
    void *cpu_out=&fake;uint32_t owned_out=99;
    bad=*b;bad.acquire=no_cpu;
    assert(clbb_execution_acquire_v1(&bad,&cpu_out,&owned_out)==-3 && cpu_out==&fake && owned_out==99);
    bad.acquire=bad_ownership;
    assert(clbb_execution_acquire_v1(&bad,&cpu_out,&owned_out)==-1 && owned_out==99);
    assert(clbb_execution_acquire_v1(b,nullptr,&owned_out)==-2);
    assert(clbb_execution_acquire_v1(b,&cpu_out,nullptr)==-2);
    assert(clbb_execution_release_v1(b,&fake,0)==0 && released==0);
    assert(clbb_execution_release_v1(b,&fake,2)==-2);
    std::vector<std::thread> threads;
    for(int i=0;i<8;i++)threads.emplace_back([]{
        auto *cpu=GuestCpu::get();assert(cpu==GuestCpu::get());
        for(int i=0;i<100;i++)assert(cpu->call(2)==-1234567);
        unsigned char stack[9]={0xab};stack[8]=0xcd;
        assert(cpu->call(3,11,12,13,14,15,16,17,18,reinterpret_cast<char *>(stack),9)==-7);
        assert(cpu->call64(4,0,0,0,0,0,0,0,0,nullptr,0)==UINT64_C(0xfedcba9876543210));
    });
    threads.emplace_back([] {borrowed=true;assert(GuestCpu::get()->call(1)==-1234567);});
    for(auto &thread:threads)thread.join();
    assert(acquired==8 && released==8 && calls==1609);
    puts("CLBB_EXECUTION_CORE_OK=version_shape_errors_8args_stack_wide_return_reentry_9threads_owned_borrowed");
}
