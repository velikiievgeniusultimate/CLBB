#!/usr/bin/env python3
"""Check opt-in native startup spans without building/loading the runtime."""
from pathlib import Path
from collections import Counter
import os
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
NATIVE = ROOT/'src/nb-qemu/libnb-qemu'


def extract(path, signature):
    text=path.read_text(); start=text.index(signature);end=text.index('{',start)+1;depth=1
    while depth:
        depth+=(text[end]=='{')-(text[end]=='}');end+=1
    return text[start:end]


code = r'''
#include "ClbbStartupTrace.h"
#include <cassert>
#include <atomic>
#include <string>
#include <thread>
#include <vector>
static unsigned clock_calls;
extern "C" int __real_clock_gettime(clockid_t,struct timespec*);
extern "C" int __wrap_clock_gettime(clockid_t c,struct timespec*t){
 __atomic_add_fetch(&clock_calls,1,__ATOMIC_RELAXED);int result=__real_clock_gettime(c,t);errno=EDOM;return result;
}
static int calls;
class Trampoline {
 public:virtual ~Trampoline()=default;
 protected:virtual void call(void*ret,void**args){assert(args==nullptr);calls++;*(int*)ret=0x10006;errno=ENOTTY;}
};
class JNILoadTrampoline:public Trampoline {
 public:JNILoadTrampoline(bool onload,uint64_t id):startup_library_id_(id),startup_is_on_load_(onload){}
 void invoke(int*ret){call(ret,nullptr);}
 protected:void call(void*,void**)override;
 private:uint64_t startup_library_id_;bool startup_is_on_load_;
};
''' + extract(NATIVE/'Trampoline.cpp', 'void JNILoadTrampoline::call(') + '\n' \
    + extract(NATIVE/'Trampoline.cpp', 'static uint8_t clbb_unity_startup_kind(') + '\n' \
    + extract(NATIVE/'Trampoline.cpp', 'static const char *clbb_unity_startup_begin(') + '\n' \
    + extract(NATIVE/'Trampoline.cpp', 'struct ClbbUnityStartupEnd {') + ';\n' + r'''
static void sample_span(){
 errno=ERANGE;{ClbbStartupSpan s("fixture.begin","fixture.end",42);assert(errno==ERANGE);errno=EPIPE;}assert(errno==EPIPE);
}
int main(){
 const bool active=clbb_startup_trace_enabled();
 errno=E2BIG;assert(clbb_startup_trace_enabled()==active&&errno==E2BIG);
 uint64_t id=clbb_startup_library_id("/private/account@example.test/libunity.so");
 assert(id==clbb_startup_library_id("/different/person/libunity.so"));
 assert(!active||id!=clbb_startup_library_id("/different/person/libmain.so"));
 assert(errno==E2BIG);
 sample_span();int ret=0;
 JNILoadTrampoline onload(true,id),unload(false,id);
 onload.invoke(&ret);assert(ret==0x10006&&calls==1&&errno==ENOTTY);
 unload.invoke(&ret);assert(ret==0x10006&&calls==2&&errno==ENOTTY);
 std::vector<std::thread> workers;
 for(int i=0;i<4;i++)workers.emplace_back([]{for(int j=0;j<8;j++)sample_span();});
 for(auto&t:workers)t.join();
 const char* names[]={"nativeInit","com.unity3d.player.UnityPlayer::nativeInitCallback::V","Java_com_unity3d_player_UnityPlayer_nativeRender"};
 for(unsigned k=1;k<=3;k++){
  const uint8_t kind=clbb_unity_startup_kind(names[k-1]);assert(kind==(active?k:0));
  std::atomic<unsigned> observed{0};workers.clear();
  for(int i=0;i<12;i++)workers.emplace_back([&]{for(int j=0;j<16;j++){
   errno=EFAULT;{ClbbUnityStartupEnd span{clbb_unity_startup_begin(kind,observed)};assert(errno==EFAULT);errno=EAGAIN;}assert(errno==EAGAIN);
  }});
  for(auto&t:workers)t.join();
  unsigned previous=clock_calls;
  for(int i=0;i<1000;i++)assert(clbb_unity_startup_begin(kind,observed)==nullptr);
  assert(previous==clock_calls);
 }
 assert(clbb_unity_startup_kind("unrelated::nativeRender::Z")==0);
 assert(clbb_unity_startup_kind("Java_com_unity3d_player_UnityPlayer_nativeInitExtra")==0);
 // Each separately constructed trampoline gets its own budget.
 std::atomic<unsigned> second_render{0};
 for(int i=0;i<8;i++){ClbbUnityStartupEnd span{clbb_unity_startup_begin(clbb_unity_startup_kind(names[2]),second_render)};}
 assert(clock_calls==(active?184u:0u));
 std::printf("STARTUP_NATIVE_TRACE_FIXTURE_OK enabled=%d clocks=%u calls=%d\n",active,clock_calls,calls);
}
'''
with tempfile.TemporaryDirectory(prefix='clbb-native-startup-trace-') as temporary:
    directory=Path(temporary);source=directory/'check.cpp';binary=directory/'check'
    source.write_text(code)
    subprocess.run(['c++','-std=c++17','-O2','-Wall','-Wextra','-Werror','-pthread',
                    '-fsanitize=undefined','-fno-sanitize-recover=all','-I'+str(NATIVE),
                    str(source),'-Wl,--wrap=clock_gettime','-o',str(binary)],check=True)
    for value in (None,'0','false','true','01','1'):
        env=dict(os.environ);env.pop('CLBB_STARTUP_TRACE',None)
        if value is not None:env['CLBB_STARTUP_TRACE']=value
        result=subprocess.run([str(binary)],env=env,check=True,capture_output=True,text=True,timeout=5)
        if value!='1':
            assert not result.stderr and 'clocks=0' in result.stdout
        else:
            lines=result.stderr.splitlines()
            assert len(lines)==92
            events=Counter()
            for line in lines:
                match=re.fullmatch(r'\[CLBB startup\] event=([A-Za-z_.]+) mono_ns=(\d+) cpu_ns=(\d+) pid=(\d+) tid=(\d+) library_id=([0-9a-f]{16})',line)
                assert match,line
                event,mono,cpu,pid,tid,library_id=match.groups()
                assert int(mono)>0 and int(cpu)>0 and int(pid)>0 and int(tid)>0
                assert '@' not in line and '/private' not in line and 'libunity' not in line
                events[event]+=1
            assert events=={'fixture.begin':33,'fixture.end':33,'JNI_OnLoad.begin':1,'JNI_OnLoad.end':1,
                           'unity.nativeInit.begin':2,'unity.nativeInit.end':2,
                           'unity.nativeInitCallback.begin':2,'unity.nativeInitCallback.end':2,
                           'unity.nativeRender.begin':8,'unity.nativeRender.end':8}
    # Make markers' exact placement part of the regression: native load includes
    # lock contention, guest spans cover QemuCpu work, and cached loads return.
    load=extract(NATIVE/'QemuBridge.cpp','void *QemuBridgeImpl::load_library(')
    assert load.index('ClbbStartupSpan startup(')<load.index('std::lock_guard')
    assert load.index('ClbbStartupSpan guest(')<load.index('ret = GuestCpu::get()->call(load_library_')
    assert 'clbb_startup_library_id(lib->get_name().c_str())' in (NATIVE/'QemuBridge.cpp').read_text()
    constructor=extract(NATIVE/'Trampoline.cpp','Trampoline::Trampoline(')
    assert 'startup_unity_kind_(clbb_unity_startup_kind(name))' in constructor
    call=extract(NATIVE/'Trampoline.cpp','void Trampoline::call(void *ret, void **args)')
    assert 'clbb_unity_startup_begin(startup_unity_kind_, startup_unity_calls_)' in call
print('STARTUP_NATIVE_TRACE_OK strict_opt_in=1 default_clocks=0 errno_preserved=1 onload_actual_override=1 unity_budgets=2,2,4 race_threads=12 ubsan=1')
