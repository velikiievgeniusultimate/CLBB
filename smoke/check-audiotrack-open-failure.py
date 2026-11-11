#!/usr/bin/env python3
"""Production JNI failure contract: shared mixer unavailable, no state writes.
Min-buffer query is pure and must not open another desktop output.
"""
import os,subprocess,tempfile
from pathlib import Path
root=Path(__file__).resolve().parents[1]
text=(root/'src/atl/src/api-impl-jni/audio/android_media_AudioTrack.c').read_text()
def extract(name):
 start=text.index('JNIEXPORT',text.index(name)-40);opening=text.index('{',start);end=opening+1;depth=1
 while depth:
  depth+=(text[end]=='{')-(text[end]=='}');end+=1
 return text[start:end]
code=r'''
#include <jni.h>
#include <stdint.h>
#include <dlfcn.h>
#include <assert.h>
#include <stdio.h>
static int opens,writes,throws;
unsigned clbb_mixer_open(unsigned rate,unsigned channels,unsigned capacity,void *callback,void *user){assert(rate==48000&&channels==2&&capacity==1024&&!callback&&!user);opens++;return 0;}
#define _SET_LONG_FIELD(o,n,v) (++writes)
#define _SET_INT_FIELD(o,n,v) (++writes)
static jclass find_class(JNIEnv *e,const char *n){(void)e;(void)n;return (jclass)(uintptr_t)1;}
static jint throw_new(JNIEnv *e,jclass c,const char *m){(void)e;(void)c;assert(m);throws++;return 0;}
static const struct JNINativeInterface_ table={.FindClass=find_class,.ThrowNew=throw_new};
'''+extract('Java_android_media_AudioTrack_native_1constructor')+'\n'+extract('Java_android_media_AudioTrack_getMinBufferSize')+r'''
int main(void){JNIEnv e=&table;
 for(int i=0;i<32;i++){
  assert(Java_android_media_AudioTrack_getMinBufferSize(&e,0,48000,12,2)==3840);
  assert(opens==i);
  Java_android_media_AudioTrack_native_1constructor(&e,0,3,48000,12,2,4096,1);
  assert(opens==i+1&&throws==i+1&&writes==0);
 }
 assert(Java_android_media_AudioTrack_getMinBufferSize(&e,0,0,12,2)==-2);
 puts("AUDIOTRACK_OPEN_FAILURE_OK pure_buffer_query no_extra_output unavailable_mixer_no_state_writes");}
'''
with tempfile.TemporaryDirectory(prefix='clbb-track-failure-') as d:
 p=Path(d);(p/'check.c').write_text(code)
 java=Path(subprocess.check_output(['readlink','-f',subprocess.check_output(['which','javac'],text=True).strip()],text=True).strip()).parents[1]
 subprocess.run(['cc','-rdynamic','-I'+str(java/'include'),'-I'+str(java/'include/linux'),str(p/'check.c'),'-ldl','-o',str(p/'check')],check=True)
 subprocess.run([str(p/'check')],check=True)
