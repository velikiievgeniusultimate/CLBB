#!/usr/bin/env python3
"""Compile production JNI bodies with field adapters and an actual GTK clock."""
import os
import pathlib
import shlex
import subprocess
import tempfile

root = pathlib.Path(__file__).resolve().parents[1]
source = 'src/api-impl-jni/views/android_view_ViewTreeObserver.c'
adapter = r'''
#include <gtk/gtk.h>
#include <jni.h>
#include <stdint.h>
#include <string.h>
typedef struct { jlong window, layout, predraw; unsigned layouts, draws; } Observer;
static jboolean no_exception(JNIEnv *env) { (void)env; return JNI_FALSE; }
static const struct JNINativeInterface_ functions = { .ExceptionCheck = no_exception };
static JNIEnv environment = &functions;
static JNIEnv *get_jni_env(void) { return &environment; }
static jlong *field(jobject object, const char *name) {
    Observer *o = (Observer *)object;
    return !strcmp(name, "window") ? &o->window :
           !strcmp(name, "onGlobalLayout_signal_handle") ? &o->layout : &o->predraw;
}
#define _PTR(value) ((void *)(intptr_t)(value))
#define _GET_LONG_FIELD(object,name) (*field(object,name))
#define _SET_LONG_FIELD(object,name,value) (*field(object,name) = (value))
#define _REF(object) (object)
#define J__ViewTreeObserver__dispatchOnGlobalLayout(env,object) (((Observer *)(object))->layouts++)
#define J__ViewTreeObserver__dispatchOnPreDraw(env,object) (((Observer *)(object))->draws++)
'''
fixture = r'''
int main(void) {
    gtk_init();
    GtkWidget *widget = gtk_window_new();
    gtk_window_present(GTK_WINDOW(widget));
    GdkFrameClock *clock = gtk_widget_get_frame_clock(widget);
    g_assert_nonnull(clock);
    Observer observer = { .window = (intptr_t)widget };
    jobject object = (jobject)&observer;
    for (unsigned i = 0; i < 200; ++i) {
        Java_android_view_ViewTreeObserver_native_1set_1have_1global_1layout_1listeners(&environment,object,JNI_FALSE);
        Java_android_view_ViewTreeObserver_native_1set_1have_1predraw_1listeners(&environment,object,JNI_FALSE);
        for (unsigned j = 0; j < 2; ++j) {
            Java_android_view_ViewTreeObserver_native_1set_1have_1global_1layout_1listeners(&environment,object,JNI_TRUE);
            Java_android_view_ViewTreeObserver_native_1set_1have_1predraw_1listeners(&environment,object,JNI_TRUE);
        }
        g_signal_emit_by_name(clock,"layout");
        g_signal_emit_by_name(clock,"paint");
        g_assert_cmpuint(observer.layouts, ==, i+1);
        g_assert_cmpuint(observer.draws, ==, i+1);
        for (unsigned j = 0; j < 2; ++j) {
            Java_android_view_ViewTreeObserver_native_1set_1have_1global_1layout_1listeners(&environment,object,JNI_FALSE);
            Java_android_view_ViewTreeObserver_native_1set_1have_1predraw_1listeners(&environment,object,JNI_FALSE);
        }
        g_signal_emit_by_name(clock,"layout");
        g_signal_emit_by_name(clock,"paint");
        g_assert_cmpuint(observer.layouts, ==, i+1);
        g_assert_cmpuint(observer.draws, ==, i+1);
    }
    gtk_window_destroy(GTK_WINDOW(widget));
    puts("OBSERVER_IDEMPOTENCE_OK cycles=200 phases=2 duplicate_add_remove=1");
}
'''
flags = shlex.split(subprocess.check_output(['pkg-config', '--cflags', '--libs', 'gtk4'], text=True))
env = os.environ.copy()
env.update(GTK_A11Y='none', GDK_BACKEND='x11', GSK_RENDERER='cairo')
with tempfile.TemporaryDirectory(prefix='clbb-observer-') as temporary:
    directory = pathlib.Path(temporary)
    for name, contents in [('baseline', subprocess.check_output(['git', 'show', 'HEAD:' + source], cwd=root/'src/atl', text=True)),
                           ('fixed', (root/'src/atl'/source).read_text())]:
        # JNI dependencies are adapted; production native function bodies stay intact.
        body = '\n'.join(line for line in contents.splitlines() if not line.startswith('#include'))
        path = directory / (name + '.c')
        path.write_text(adapter + body + fixture)
        binary = directory / name
        subprocess.run(['cc', '-std=c11', '-Werror', '-I/usr/lib/jvm/java-21-openjdk/include',
                        '-I/usr/lib/jvm/java-21-openjdk/include/linux', str(path), *flags, '-o', str(binary)], check=True)
        result = subprocess.run(['timeout', '10s', 'xvfb-run', '-a', str(binary)], env=env, capture_output=True, text=True)
        if name == 'baseline':
            assert result.returncode == 1 and 'invalid state' in result.stderr, result
        else:
            assert result.returncode == 0, result.stderr
            print(result.stdout.strip())
    print('BASELINE_REPRODUCES_EXIT_1=1')
