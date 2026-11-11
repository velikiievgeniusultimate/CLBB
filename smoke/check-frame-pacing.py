#!/usr/bin/env python3
"""Exercise actual Rust FFI and production C/Java pacing seams, without a game.

Only a private process's in-memory pacing configuration is changed. Android OS,
GL and queue stubs make the adapters deterministic; Rust sleeps use wall time.
No Runtime object, settings file, display server or installed library is used.
"""
from concurrent.futures import ThreadPoolExecutor
import argparse
import ctypes
import json
from pathlib import Path
import shutil
import statistics
import subprocess
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[1]


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def rust_check(library):
    lib = ctypes.CDLL(str(library))  # CDLL releases the GIL during foreign calls.
    configure = lib.clbb_frame_pacer_configure
    configure.argtypes = [ctypes.c_uint, ctypes.c_int]
    configure.restype = ctypes.c_int
    read = lib.clbb_frame_pacer_configuration
    read.argtypes = []
    read.restype = ctypes.c_uint
    wait = lib.clbb_frame_pacer_wait
    wait.argtypes = []
    wait.restype = None
    original = read()
    require(original == (60 | (1 << 16)), "fresh library defaults must be 60 FPS with VSync")
    results = {}

    def elapsed_wait():
        start = time.monotonic()
        wait()
        return time.monotonic() - start

    try:
        for fps in (30, 60, 120, 240):
            require(configure(fps, 0) == 1, f"configure {fps}")

            def samples():
                wait()  # Each fresh producer starts immediately.
                return [elapsed_wait() for _ in range(12)]

            with ThreadPoolExecutor(max_workers=1) as pool:
                values = pool.submit(samples).result(timeout=3)
            period = 1 / fps
            # Loose upper bounds tolerate a busy build host; lower bounds catch
            # missing sleeps, old fixed limits and wrong units at every rate.
            require(min(values) >= period * .97, f"{fps}: cap exceeded: {values}")
            require(statistics.median(values) < period * 2 + .005,
                    f"{fps}: unexpected pacing slowdown: {values}")
            results[str(fps)] = {"median_ms": round(statistics.median(values) * 1000, 3),
                                 "min_ms": round(min(values) * 1000, 3)}

        require(configure(30, 1) == 1, "configure isolation")
        barrier = threading.Barrier(4)

        def producer():
            barrier.wait(timeout=2)
            start = time.monotonic()
            wait()
            for _ in range(6):
                wait()
            return time.monotonic() - start

        with ThreadPoolExecutor(max_workers=4) as pool:
            futures = [pool.submit(producer) for _ in range(4)]
            durations = [f.result(timeout=3) for f in futures]
        # Four independent 200 ms schedules must not become one 800 ms queue.
        require(min(durations) >= .19, f"producer missed cap: {durations}")
        require(max(durations) < .50, f"producers serialized: {durations}")
        results["independent_producers_ms"] = [round(d * 1000, 3) for d in durations]

        def transitions():
            require(configure(30, 0) == 1, "transition start")
            wait()
            require(configure(240, 0) == 1, "FPS transition")
            require(elapsed_wait() < .020, "FPS edit retained old 33ms deadline")
            require(elapsed_wait() >= 1 / 240 * .97, "new FPS not enforced")
            require(configure(30, 0) == 1, "vsync transition start")
            wait()
            require(configure(30, 1) == 1, "vsync transition")
            require(elapsed_wait() < .020, "VSync edit retained old deadline")
            require(elapsed_wait() >= 1 / 30 * .97, "VSync reset lost FPS cap")
            previous = read()
            for fps, vsync in ((0, 1), (29, 1), (241, 1), (2**32-1, 1),
                               (60, -1), (60, 2)):
                require(configure(fps, vsync) == 0, f"accepted invalid {(fps, vsync)}")
                require(read() == previous, "invalid config changed previous value")
            # Reapplying the same config must not continually clear the cap.
            require(configure(30, 1) == 1, "same config")
            require(elapsed_wait() >= .030, "same/invalid config reset deadline")
            require(configure(120, 0) == 1, "late producer config")
            wait()
            time.sleep(.045)
            require(elapsed_wait() < .020, "late producer waits an extra period")
            require(elapsed_wait() >= 1 / 120 * .97, "late producer catch-up burst")

        with ThreadPoolExecutor(max_workers=1) as pool:
            pool.submit(transitions).result(timeout=3)
        results["config_reset_invalid_preserved_no_catchup"] = True
    finally:
        require(configure(original & 0xffff, int(bool(original & (1 << 16)))) == 1,
                "restore private process config")
    return results


def extract(relative, signature):
    text = (ROOT / relative).read_text()
    start = text.index(signature)
    opening = text.index("{", start)
    depth, end = 1, opening + 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[start:end]


def native_check(directory):
    host = 'src/atl/src/main-executable/clbb_mapper.c'
    egl = 'src/atl/src/libandroid/egl.c'
    code = r'''
#define _POSIX_C_SOURCE 200809L
#include <assert.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
typedef void (*ClbbFrameWait)(void);
typedef unsigned int (*ClbbFrameConfiguration)(void);
static _Atomic(ClbbFrameWait) frame_wait;
static _Atomic(ClbbFrameConfiguration) frame_configuration;
extern void clbb_frame_pacing_wait(void) __attribute__((weak));
extern void clbb_boot_event(const char *) __attribute__((weak));
extern int clbb_frame_pacing_vsync(void) __attribute__((weak));
''' + extract(host, 'void clbb_frame_pacing_wait(void)') + '\n' + extract(host, 'int clbb_frame_pacing_vsync(void)') + r'''
typedef void *EGLDisplay; typedef void *EGLSurface;
typedef int EGLBoolean; typedef unsigned GLuint;
#define EGL_TRUE 1
#define G_PRIORITY_HIGH_IDLE 100
struct atl_surface_buffer { GLuint gl_framebuffer; unsigned submit_serial; } buffer={7,0}, next_buffer={9,0};
typedef struct { struct atl_surface_buffer *back_buffer; void *vsync,*unused_buffers; unsigned submitted_frames; } ATLSurface;
struct ANativeWindow { int unused; } native_window;
static int owned=1, waits, config, direct, tokens, dequeues, serial_at_idle, traces, flushes;
static void *egl_surface_hashtable;
static void *g_hash_table_lookup(void *table,void *key){(void)table;(void)key;return owned?&native_window:NULL;}
static EGLBoolean eglSwapBuffers(EGLDisplay d,EGLSurface s){(void)d;(void)s;direct++;return EGL_TRUE;}
static void trace_swap(ATLSurface*s){assert(s->back_buffer==&buffer);traces++;}
static void glFinish(void){}
static void glFlush(void){flushes++;}
static void *g_async_queue_timeout_pop(void *q,unsigned long us){if(q==(void*)1){assert(us==50000);tokens++;return NULL;}assert(q==(void*)2&&us==100000);dequeues++;return &next_buffer;}
static void g_object_ref(void *s){(void)s;}
static int queue_texture(void *p){(void)p;return 0;}
static int g_idle_add_full(int p,int(*f)(void*),void*b,void*d){assert(p==120&&f==queue_texture&&b==&buffer&&!d);serial_at_idle=buffer.submit_serial;return 1;}
static void rotate_default_framebuffer(GLuint old,GLuint next){assert(old==7&&next==9);}
''' + extract(egl, 'EGLBoolean bionic_eglSwapBuffers(') + r'''
static void count_wait(void){waits++;}
static unsigned read_config(void){return (unsigned)config;}
int main(void){
  unsetenv("ATL_DIRECT_EGL");unsetenv("ATL_EGL_SYNC_DIAGNOSTIC");
  clbb_frame_pacing_wait();assert(clbb_frame_pacing_vsync()==1);
  atomic_store(&frame_wait,count_wait);atomic_store(&frame_configuration,read_config);
  for(int sync=0;sync<=1;sync++){
    config=120|(sync<<16);ATLSurface s={&buffer,(void*)1,(void*)2,0};
    waits=tokens=dequeues=traces=flushes=0;buffer.submit_serial=0;
    assert(bionic_eglSwapBuffers(NULL,&s)==EGL_TRUE);
    assert(waits==1&&tokens==sync&&dequeues==1&&traces==1&&flushes==1);
    assert(s.back_buffer==&next_buffer&&s.submitted_frames==1&&serial_at_idle==1);
  }
  waits=0;owned=0;assert(bionic_eglSwapBuffers(NULL,NULL)==EGL_TRUE);assert(direct==1&&!waits);
  owned=1;setenv("ATL_DIRECT_EGL","1",1);assert(bionic_eglSwapBuffers(NULL,NULL)==EGL_TRUE);assert(direct==2&&!waits);
  puts("FRAME_PACING_NATIVE_OK default_optional_callbacks=1 egl_wait=1 vsync_toggle=1 buffer_ownership=1 direct_fallback=1");
}
'''
    source = directory / 'native.c'
    source.write_text(code)
    binary = directory / 'native'
    # Host and EGL normally link from different objects. In this combined
    # fixture their optional weak functions are visibly defined to the compiler.
    subprocess.run(['cc', '-std=c11', '-O1', '-g', '-Wall', '-Wextra', '-Werror', '-Wno-address',
                    '-fsanitize=undefined', '-fno-sanitize-recover=all',
                    str(source), '-o', str(binary)], check=True)
    subprocess.run([str(binary)], check=True, timeout=5)


def java_check(directory):
    # Compile the complete production classes; only Android OS infrastructure is
    # stubbed. Queue dispatch then calls the real FrameHandler and doFrame.
    sources = {
        'android/atl/annotation/Export.java': 'package android.atl.annotation; public @interface Export {}',
        'android/graphics/Point.java': 'package android.graphics; public class Point {public int x,y;public void set(int a,int b){x=a;y=b;}}',
        'android/graphics/Rect.java': 'package android.graphics; public class Rect {public void set(int a,int b,int c,int d){}}',
        'android/util/DisplayMetrics.java': 'package android.util; public class DisplayMetrics {public int widthPixels,heightPixels;}',
        'android/content/res/Resources.java': 'package android.content.res; public class Resources {public static Resources getSystem(){return new Resources();}public android.util.DisplayMetrics getDisplayMetrics(){return new android.util.DisplayMetrics();}}',
        'android/view/DisplayCutout.java': 'package android.view; public class DisplayCutout {public static final DisplayCutout NO_CUTOUT=new DisplayCutout();}',
        'android/os/Looper.java': 'package android.os; public class Looper {static final Looper INSTANCE=new Looper();public static Looper myLooper(){return INSTANCE;}}',
        'android/os/SystemClock.java': 'package android.os; public class SystemClock {public static long now=1000;public static long uptimeMillis(){return now;}}',
        'android/util/Log.java': 'package android.util; public class Log {public static int d(String a,String b){return 0;}public static int i(String a,String b){return 0;}}',
        'android/os/Message.java': 'package android.os; public class Message {public int what,arg1;public Object obj;public boolean async;public void setAsynchronous(boolean b){async=b;}}',
        'android/os/Handler.java': '''package android.os; public class Handler {
public static Handler owner;public static Message pending;public static long time;
public Handler(Looper l){} public Message obtainMessage(int what){return obtainMessage(what,null);}
public Message obtainMessage(int what,Object obj){Message m=new Message();m.what=what;m.obj=obj;return m;}
public boolean sendMessageAtTime(Message m,long t){owner=this;pending=m;time=t;return true;}
public void removeMessages(int w,Object o){} public void handleMessage(Message m){}
public static void dispatch(){Message m=pending;pending=null;owner.handleMessage(m);}
}''',
        'FramePacingContract.java': r'''
import android.view.Display;
import android.view.Choreographer;
import android.os.Handler;
import android.os.SystemClock;
import java.lang.reflect.Field;
public final class FramePacingContract {
 static void check(boolean v,String s){if(!v)throw new AssertionError(s);}
 public static void main(String[] args)throws Exception{
  Display d=new Display();Display.Mode retained=d.getMode();
  Display.setWindowSize(4000,3111);
  Choreographer c=Choreographer.getInstance();
  Field last=Choreographer.class.getDeclaredField("mLastFrameTimeNanos");last.setAccessible(true);
  Field interval=Choreographer.class.getDeclaredField("mFrameIntervalNanos");interval.setAccessible(true);
  for(int fps:new int[]{30,60,90,120,144,165,180,240}){
   Display.setFrameRateLimit(fps);long delay=1000L/fps;
   check(Display.getFrameRateLimit()==fps&&d.getRefreshRate()==fps,"Display FPS");
   check(d.getMode().getRefreshRate()==fps&&retained.getRefreshRate()==fps,"Mode FPS");
   check(d.getSupportedModes().length==1&&d.getSupportedModes()[0].getRefreshRate()==fps,"supported Mode");
   check(retained.getPhysicalWidth()==4000&&retained.getPhysicalHeight()==3111,"resolution retained");
   check(Choreographer.getFrameDelay()==delay,"callback delay");
   check(Choreographer.subtractFrameDelay(delay+7)==7,"subtract delay");
   last.setLong(c,SystemClock.now*1000000L);int[] callbacks={0};
   c.postFrameCallback(t->callbacks[0]++);
   check(Handler.pending!=null&&Handler.pending.async&&Handler.time==SystemClock.now+delay,"actual scheduled callback delay");
   SystemClock.now=Handler.time;Handler.dispatch();
   check(callbacks[0]==1,"actual callback dispatch");
   check(interval.getLong(c)==(long)(1000000000/(float)fps),"live Choreographer interval");
  }
  long previous=Choreographer.getFrameDelay();
  for(int invalid:new int[]{Integer.MIN_VALUE,-1,0,29,241,Integer.MAX_VALUE}){
   try{Display.setFrameRateLimit(invalid);throw new AssertionError("accepted invalid FPS");}catch(IllegalArgumentException expected){}
   check(Display.getFrameRateLimit()==240&&retained.getRefreshRate()==240&&Choreographer.getFrameDelay()==previous,"invalid preserves config");
  }
  System.out.println("FRAME_PACING_JAVA_OK production_Display_Choreographer=1 modes=1 live_callback_delay=1 callback_dispatch=1 invalid_preserved=1");
 }
}''',
    }
    paths = []
    for relative, text in sources.items():
        path = directory / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        paths.append(str(path))
    paths.extend(str(ROOT / 'src/atl/src/api-impl/android/view' / f'{name}.java')
                 for name in ('Display', 'Choreographer'))
    subprocess.run(['javac', '-encoding', 'UTF-8', '-d', str(directory), *paths], check=True, timeout=25)
    subprocess.run(['java', '-cp', str(directory), 'FramePacingContract'], check=True, timeout=8)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--library', type=Path,
                        default=ROOT / 'mapper/runtime/target/release/libclbb_mapper.so')
    args = parser.parse_args()
    for executable in ('cc', 'javac', 'java'):
        require(shutil.which(executable), f'{executable} is required')
    require(args.library.is_file(), f'Build release mapper library first: {args.library}')
    results = rust_check(args.library.resolve())
    with tempfile.TemporaryDirectory(prefix='clbb-pacing-check-') as tmp:
        directory = Path(tmp)
        native_check(directory)
        java_check(directory)
    print('FRAME_PACING_RUST_OK ' + json.dumps(results, sort_keys=True), flush=True)


if __name__ == '__main__':
    main()
