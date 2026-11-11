#!/usr/bin/env python3
"""Check production native host seams in an isolated GTK display, no game input."""
from pathlib import Path
import os,subprocess,tempfile
root=Path(__file__).resolve().parents[1]
def extract(path,start):
 text=(root/path).read_text();i=text.index(start);opening=text.index('{',i);depth=1;j=opening+1
 while depth:
  depth+=(text[j]=='{')-(text[j]=='}');j+=1
 return text[i:j]
activity=extract('src/atl/src/api-impl-jni/app/android_app_Activity.c','GtkStack *activity_stack(void)')
action=extract('src/atl/src/api-impl-jni/views/android_view_View.c','static int clbb_pointer_action(int edge,int count,int index)\n{')
audio=extract('src/atl/src/api-impl-jni/audio/android_media_AudioTrack.c','static int write_frames(')
sles=extract('src/libopensles-standalone/libopensles/SDL.c','static void SDL_mix(')
code=r'''
#include <gtk/gtk.h>
#include <assert.h>
#include <errno.h>
#include <dlfcn.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
GtkWindow *window;
enum {ACTION_DOWN=0,ACTION_UP=1,ACTION_MOVE=2,ACTION_POINTER_DOWN=5,ACTION_POINTER_UP=6};
typedef short jshort;
typedef struct {int unused;} snd_pcm_t;
typedef long snd_pcm_sframes_t;
static int writes,underrun;static short output[8];
static long snd_pcm_writei(snd_pcm_t*p,const void*b,int n){(void)p;if(++writes==1&&underrun)return -EPIPE;memcpy(output,b,n*sizeof(short));return n;}
static int snd_pcm_recover(snd_pcm_t*p,long e,int s){(void)p;(void)e;(void)s;return 0;}
static int snd_pcm_start(snd_pcm_t*p){(void)p;return 0;}
static const char *snd_strerror(long e){(void)e;return "test";}
float clbb_master_gain(void){return .5f;}
typedef uint32_t SLuint32;
#define STEREO_CHANNELS 2
typedef float (*ClbbMasterGain)(void);
static _Atomic(ClbbMasterGain) clbb_sles_gain;
'''+activity+'\n'+action+'\n'+audio+'\n'+sles.replace('clbb_master_gain, memory_order_relaxed','clbb_sles_gain, memory_order_relaxed')+r'''
int main(void){
 gtk_init();window=GTK_WINDOW(gtk_window_new());GtkWidget *stack=gtk_stack_new();gtk_window_set_child(window,stack);assert(activity_stack()==GTK_STACK(stack));
 g_object_set_data(G_OBJECT(window),"atl-activity-stack",stack);g_object_ref(stack);gtk_window_set_child(window,NULL);GtkWidget*overlay=gtk_overlay_new();gtk_overlay_set_child(GTK_OVERLAY(overlay),stack);g_object_unref(stack);gtk_window_set_child(window,overlay);assert(activity_stack()==GTK_STACK(stack));
 GtkWidget*scene=gtk_label_new("scene");gtk_stack_add_child(activity_stack(),scene);gtk_stack_set_visible_child(activity_stack(),scene);assert(gtk_stack_get_visible_child(activity_stack())==scene);
 assert(clbb_pointer_action(0,1,0)==ACTION_DOWN);assert(clbb_pointer_action(0,2,1)==(ACTION_POINTER_DOWN|256));assert(clbb_pointer_action(2,2,0)==ACTION_POINTER_UP);assert(clbb_pointer_action(2,2,1)==(ACTION_POINTER_UP|256));assert(clbb_pointer_action(2,1,0)==ACTION_UP);assert(clbb_pointer_action(1,2,1)==ACTION_MOVE);
 short input[]={1000,-1000,7777};assert(write_frames(NULL,input,2,1,.5f)==2);assert(output[0]==250&&output[1]==-250);assert(input[0]==1000&&input[1]==-1000&&input[2]==7777);
 writes=0;underrun=1;assert(write_frames(NULL,input,2,1,.5f)==2);assert(writes==2&&output[0]==250&&input[0]==1000);
 atomic_store(&clbb_sles_gain,clbb_master_gain);short out[2];float gains[]={.8f,.4f};SDL_mix(input,out,4,2,gains);assert(out[0]==400&&out[1]==-200);assert(input[0]==1000);
 gtk_window_destroy(window);puts("MAPPER_HOST_OK overlay_activity_identity=1 physical_primary_up=1 master_gain_both_outputs=1 java_pcm_immutable=1");return 0;
}
'''
with tempfile.TemporaryDirectory(prefix='clbb-host-check-') as directory:
 d=Path(directory);source=d/'check.c';source.write_text(code);exe=d/'check'
 flags=subprocess.check_output(['pkg-config','--cflags','--libs','gtk4'],text=True).split()
 subprocess.run(['cc','-O1','-g','-rdynamic',str(source),'-o',str(exe),*flags,'-ldl'],check=True)
 display=d/'display'
 with display.open('w') as out, (d/'xvfb.log').open('w') as log:
  x=subprocess.Popen(['Xvfb','-displayfd',str(out.fileno()),'-screen','0','800x600x24','-nolisten','tcp'],pass_fds=(out.fileno(),),stdout=log,stderr=log)
  try:
   import time
   for _ in range(100):
    if display.stat().st_size:break
    time.sleep(.02)
   env={**os.environ,'DISPLAY':':'+display.read_text().strip(),'GDK_BACKEND':'x11','GSK_RENDERER':'cairo','GTK_A11Y':'none'}
   subprocess.run([str(exe)],env=env,check=True,timeout=10)
  finally:x.terminate();x.wait(timeout=5)
