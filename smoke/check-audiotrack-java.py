#!/usr/bin/env python3
"""Production AudioTrack direct/heap buffers and SDK metadata; no real audio output."""
from pathlib import Path
import subprocess,tempfile
root=Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='clbb-track-api-') as tmp:
 p=Path(tmp);media=p/'android/media';media.mkdir(parents=True);ann=p/'android/atl/annotation';ann.mkdir(parents=True)
 (media/'AudioTrack.java').write_bytes((root/'src/atl/src/api-impl/android/media/AudioTrack.java').read_bytes())
 (media/'AudioFormat.java').write_text('package android.media;public class AudioFormat {int sampleRate,channelMask,encoding;}')
 (media/'AudioAttributes.java').write_text('package android.media;public class AudioAttributes {int streamType;}')
 (media/'AudioTimestamp.java').write_text('package android.media;public class AudioTimestamp {}')
 (ann/'Export.java').write_text('package android.atl.annotation;public @interface Export {}')
 (p/'Check.java').write_text('''import android.media.AudioTrack;import java.nio.ByteBuffer;
public class Check {static void check(boolean b){if(!b)throw new AssertionError();}public static void main(String[] args){System.load(args[0]);
 AudioTrack a=new AudioTrack(3,44100,12,2,1920,1);check(a.getState()==1&&a.getChannelCount()==2&&a.getStreamType()==3&&a.getBufferSizeInFrames()==480);check(a.getBufferCapacityInFrames()==480);check(AudioTrack.getMaxVolume()==1&&AudioTrack.getNativeOutputSampleRate(3)==44100);
 ByteBuffer direct=ByteBuffer.allocateDirect(64);direct.position(2);check(a.write(direct,8,0)==8&&direct.position()==10);
 direct.clear();check(a.write(direct,32,1)==16&&direct.position()==16);check(a.write(direct,100,0)==-2);check(a.write(direct,1,99)==-2);
 ByteBuffer heap=ByteBuffer.wrap(new byte[32]);heap.position(3);check(a.write(heap,8,0)==8&&heap.position()==11);
 a.release();check(a.getState()==0&&a.write(direct,4,0)==-3);
 System.out.println("AUDIOTRACK_JAVA_OK direct/heap buffers, blocking/nonblocking, bounds, metadata, released state");}}
''')
 subprocess.run(['javac','-h',str(p),'-d',str(p),*[str(f)for f in p.rglob('*.java')]],check=True)
 java=Path(subprocess.check_output(['readlink','-f',subprocess.check_output(['which','javac'],text=True).strip()],text=True).strip()).parents[1]
 (p/'mock.c').write_text('''#include <jni.h>
#include "android_media_AudioTrack.h"
static void field(JNIEnv *e,jobject o,const char *name,const char *type,jlong value){jfieldID f=(*e)->GetFieldID(e,(*e)->GetObjectClass(e,o),name,type);if(*type=='J')(*e)->SetLongField(e,o,f,value);else (*e)->SetIntField(e,o,f,value);}
JNIEXPORT void JNICALL Java_android_media_AudioTrack_native_1constructor(JNIEnv *e,jobject o,jint s,jint r,jint c,jint f,jint b,jint m){field(e,o,"pcm_handle","J",1);field(e,o,"channels","I",2);}
JNIEXPORT jint JNICALL Java_android_media_AudioTrack_native_1write___3BIIF(JNIEnv *e,jobject o,jbyteArray data,jint offset,jint frames,jfloat gain){return frames;}
JNIEXPORT jint JNICALL Java_android_media_AudioTrack_native_1availableFrames(JNIEnv *e,jobject o){return 4;}
JNIEXPORT void JNICALL Java_android_media_AudioTrack_native_1release(JNIEnv *e,jobject o){field(e,o,"pcm_handle","J",0);}
''')
 subprocess.run(['cc','-shared','-fPIC','-I'+str(java/'include'),'-I'+str(java/'include/linux'),str(p/'mock.c'),'-o',str(p/'mock.so')],check=True)
 subprocess.run(['java','-cp',str(p),'Check',str(p/'mock.so')],check=True)
