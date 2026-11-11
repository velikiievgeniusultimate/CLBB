// Production registration wrapper + production Trampoline/libffi. Only guest
// execution is replaced: this test never initializes SDL, QEMU, ART or a game.
#include <atomic>
#include <cassert>
#include <cstdint>
#include <cstdio>
#include <thread>
#include <vector>
#include "libOpenSLES.itf.h"
#include "QemuCpu.h"

struct Invocation { intptr_t address = 0, args[8] = {}; };
static thread_local Invocation invoked;
QemuCpu::QemuCpu() : cpu_(nullptr), managed_(false) {}
QemuCpu::~QemuCpu() {}
QemuCpu *QemuCpu::get() { static thread_local QemuCpu cpu; return &cpu; }
intptr_t QemuCpu::call(intptr_t address, intptr_t a, intptr_t b, intptr_t c,
    intptr_t d, intptr_t e, intptr_t f, intptr_t g, intptr_t h, char *stack, int size) const
{
    assert(!stack && !size);
    invoked = {address, {a,b,c,d,e,f,g,h}};
    return 0;
}
uint64_t QemuCpu::call64(intptr_t address, intptr_t a, intptr_t b, intptr_t c,
    intptr_t d, intptr_t e, intptr_t f, intptr_t g, intptr_t h, char *stack, int size) const
{
    return call(address,a,b,c,d,e,f,g,h,stack,size);
}
extern "C" int __android_log_print(int, const char *, const char *, ...) { return 0; }

static thread_local slBufferQueueCallback queue_callback;
static thread_local slAndroidSimpleBufferQueueCallback simple_callback;
static thread_local slPlayCallback play_callback;
static thread_local slObjectCallback object_callback;
static thread_local void *context_seen;
static SLresult queue_register(SLBufferQueueItf, slBufferQueueCallback callback, void *context)
{ queue_callback=callback; context_seen=context; return SL_RESULT_SUCCESS; }
static SLresult simple_register(SLAndroidSimpleBufferQueueItf, slAndroidSimpleBufferQueueCallback callback, void *context)
{ simple_callback=callback; context_seen=context; return SL_RESULT_SUCCESS; }
static SLresult play_register(SLPlayItf, slPlayCallback callback, void *context)
{ play_callback=callback; context_seen=context; return SL_RESULT_SUCCESS; }
static SLresult object_register(SLObjectItf, slObjectCallback callback, void *context)
{ object_callback=callback; context_seen=context; return SL_RESULT_SUCCESS; }

int main()
{
    SLBufferQueueItf_ queue_table{}; queue_table.RegisterCallback=queue_register;
    SLAndroidSimpleBufferQueueItf_ simple_table{}; simple_table.RegisterCallback=simple_register;
    SLPlayItf_ play_table{}; play_table.RegisterCallback=play_register;
    SLObjectItf_ object_table{}; object_table.RegisterCallback=object_register;
    const SLBufferQueueItf_ *queue=&queue_table;
    const SLAndroidSimpleBufferQueueItf_ *simple=&simple_table;
    const SLPlayItf_ *play=&play_table;
    const SLObjectItf_ *object=&object_table;
    int context=7;
    assert(SLBufferQueueItf_RegisterCallback(&queue,nullptr,&context)==SL_RESULT_SUCCESS);
    assert(!queue_callback && context_seen==&context);
    assert(SLAndroidSimpleBufferQueueItf_RegisterCallback(&simple,nullptr,&context)==SL_RESULT_SUCCESS);
    assert(!simple_callback && context_seen==&context);
    assert(SLPlayItf_RegisterCallback(&play,nullptr,&context)==SL_RESULT_SUCCESS);
    assert(!play_callback && context_seen==&context);
    assert(SLObjectItf_RegisterCallback(&object,nullptr,&context)==SL_RESULT_SUCCESS);
    assert(!object_callback && context_seen==&context);

    constexpr unsigned threads=24, addresses=128, repeats=4;
    std::atomic<unsigned> ready{0}; std::atomic<bool> start{false};
    std::vector<std::vector<void *>> handles(threads,std::vector<void *>(addresses*3));
    std::vector<std::thread> workers;
    for(unsigned t=0;t<threads;t++) workers.emplace_back([&,t]{
        ready.fetch_add(1);
        while(!start.load()) std::this_thread::yield();
        for(unsigned repeat=0;repeat<repeats;repeat++) for(unsigned n=0;n<addresses;n++) {
            void *address=reinterpret_cast<void *>(uintptr_t(0x10000+n*16));
            assert(SLBufferQueueItf_RegisterCallback(&queue,address,&context)==SL_RESULT_SUCCESS);
            auto q=queue_callback; assert(q && context_seen==&context);
            assert(SLAndroidSimpleBufferQueueItf_RegisterCallback(&simple,address,&context)==SL_RESULT_SUCCESS);
            assert(reinterpret_cast<void *>(q)==reinterpret_cast<void *>(simple_callback));
            q(&queue,&context);
            assert(invoked.address==reinterpret_cast<intptr_t>(address));
            assert(invoked.args[0]==reinterpret_cast<intptr_t>(&queue) && invoked.args[1]==reinterpret_cast<intptr_t>(&context));
            assert(SLPlayItf_RegisterCallback(&play,address,&context)==SL_RESULT_SUCCESS);
            auto p=play_callback; assert(p && reinterpret_cast<void *>(p)!=reinterpret_cast<void *>(q));
            p(&play,&context,0x87654321u);
            assert(invoked.args[2]==0x87654321u);
            assert(SLObjectItf_RegisterCallback(&object,address,&context)==SL_RESULT_SUCCESS);
            auto o=object_callback;
            assert(o && reinterpret_cast<void *>(o)!=reinterpret_cast<void *>(q) && reinterpret_cast<void *>(o)!=reinterpret_cast<void *>(p));
            o(&object,&context,0x1234u,0x87654321u,0x76543210u,&context);
            assert(invoked.args[2]==0x1234u && invoked.args[3]==0x87654321u && invoked.args[4]==0x76543210u && invoked.args[5]==reinterpret_cast<intptr_t>(&context));
            void *current[]={reinterpret_cast<void *>(q),reinterpret_cast<void *>(p),reinterpret_cast<void *>(o)};
            for(unsigned k=0;k<3;k++) {
                auto &prior=handles[t][n*3+k]; if(prior)assert(prior==current[k]); prior=current[k];
            }
        }
    });
    while(ready.load()!=threads) std::this_thread::yield();
    start.store(true);
    for(auto &worker:workers)worker.join();
    for(unsigned t=1;t<threads;t++)assert(handles[t]==handles[0]);
    // Retained handles must survive thread exit and unrelated NULL registration.
    SLBufferQueueItf_RegisterCallback(&queue,nullptr,nullptr);
    auto saved=reinterpret_cast<slBufferQueueCallback>(handles[0][0]);
    saved(&queue,&context); assert(invoked.address==0x10000);
    puts("OPENSL_CALLBACKS_OK null=4 signatures=3 threads=24 addresses=128 repeats=4 real_trampoline=1 real_libffi=1 retained=1");
}
