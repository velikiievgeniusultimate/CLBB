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
int clbb_mixer_write(unsigned id,const short*b,int n){(void)id;++writes;memcpy(output,b,n*sizeof(short));return n;}
static int snd_pcm_recover(snd_pcm_t*p,long e,int s){(void)p;(void)e;(void)s;return 0;}
static int snd_pcm_start(snd_pcm_t*p){(void)p;return 0;}
static const char *snd_strerror(long e){(void)e;return "test";}
static float master=.5f,game=1.f,voice=.25f;
float clbb_game_gain(void){return master*game;}
float clbb_voice_gain(void){return master*voice;}
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
 short input[]={1000,-1000,7777};assert(write_frames(1,input,2,1,.5f,3)==2);assert(output[0]==250&&output[1]==-250);assert(input[0]==1000&&input[1]==-1000&&input[2]==7777);
 writes=0;underrun=1;assert(write_frames(1,input,2,1,.5f,3)==2);assert(writes==1&&output[0]==250&&input[0]==1000);
 // Received speech is independent of game volume, including SCO.
 assert(write_frames(1,input,2,1,1.f,0)==2);assert(output[0]==125);
 game=0;assert(write_frames(1,input,2,1,1.f,3)==2);assert(output[0]==0);
 assert(write_frames(1,input,2,1,1.f,0)==2);assert(output[0]==125);
 voice=0;game=1;assert(write_frames(1,input,2,1,1.f,6)==2);assert(output[0]==0);
 assert(write_frames(1,input,2,1,1.f,3)==2);assert(output[0]==500);
 master=0;assert(write_frames(1,input,2,1,1.f,0)==2);assert(output[0]==0);
 assert(write_frames(1,input,2,1,1.f,3)==2);assert(output[0]==0);master=.5f;
 atomic_store(&clbb_sles_gain,clbb_game_gain);short out[2];float gains[]={.8f,.4f};SDL_mix(input,out,4,2,gains);assert(out[0]==400&&out[1]==-200);assert(input[0]==1000);
 gtk_window_destroy(window);puts("MAPPER_HOST_OK overlay_activity_identity=1 physical_primary_up=1 independent_game_voice_master_gains=1 java_pcm_immutable=1");return 0;
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
