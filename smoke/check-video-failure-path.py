#!/usr/bin/env python3
"""Exercise production SurfaceTexture lifecycle/callbacks with its real JNI queue."""
import pathlib
import shlex
import subprocess
import tempfile

root = pathlib.Path(__file__).resolve().parents[1]
sources = root/'src/atl/src/api-impl'
stubs = {
 'android/os/Looper.java': 'package android.os; public class Looper { public static Looper getMainLooper(){return new Looper();} }',
 'android/os/Handler.java': '''package android.os;
 public class Handler {
  static java.util.ArrayDeque<Runnable> queue=new java.util.ArrayDeque<Runnable>();
  public Handler(Looper looper) {} public boolean post(Runnable r){queue.add(r);return true;}
  public static int pending(){return queue.size();} public static void drain(){while(!queue.isEmpty())queue.remove().run();}
 }''',
 'android/os/Bundle.java':'package android.os; public class Bundle {}',
 'android/media/MediaCrypto.java':'package android.media; public class MediaCrypto {}',
 'android/media/MediaFormat.java':'''package android.media; public class MediaFormat {
 public boolean containsKey(String key){return false;}public int getInteger(String key){return 0;}
 public java.nio.ByteBuffer getByteBuffer(String key){return null;} }''',
}
fixture = '''
import android.graphics.SurfaceTexture;
import android.view.Surface;
import android.media.MediaCodec;
import android.os.Handler;
import java.lang.reflect.Field;
import java.util.concurrent.atomic.AtomicInteger;
public class SurfaceTextureLifecycleSmoke {
 static native boolean offer(SurfaceTexture texture);
 static void check(boolean ok){if(!ok)throw new AssertionError();}
 public static void main(String[] args)throws Exception {
  System.load(args[0]);
  for(int cycle=0;cycle<100;cycle++) {
   SurfaceTexture texture=new SurfaceTexture(7);AtomicInteger callbacks=new AtomicInteger();
   texture.setDefaultBufferSize(1280,720);check(texture.getTimestamp()==0);
   texture.setOnFrameAvailableListener(value->{check(value==texture);callbacks.incrementAndGet();});
   check(Handler.pending()==0); // No synthetic empty-frame notifications.
   for(int i=0;i<20;i++)check(offer(texture));
   check(Handler.pending()==1&&callbacks.get()==0);Handler.drain();check(callbacks.get()==1);
   check(offer(texture));texture.setOnFrameAvailableListener(value->callbacks.addAndGet(10));
   check(offer(texture));Handler.drain();check(callbacks.get()==11); // stale listener generation skipped
   check(texture.getTimestamp()==0); // queued != latched
   float[] matrix=new float[16];texture.getTransformMatrix(matrix);check(matrix[5]==-1&&matrix[13]==1&&matrix[0]==1&&matrix[15]==1);
   try{texture.setDefaultBufferSize(0,1);throw new AssertionError();}catch(IllegalArgumentException expected){}
   try{texture.getTransformMatrix(new float[15]);throw new AssertionError();}catch(IllegalArgumentException expected){}
   Surface surface=new Surface(texture);check(surface.isValid());surface.release();surface.release();check(!surface.isValid()&&!texture.isReleased());
   check(offer(texture));texture.release();texture.release();Handler.drain();check(callbacks.get()==11&&texture.isReleased());
   try{texture.updateTexImage();throw new AssertionError();}catch(IllegalStateException expected){}
   try{offer(texture);throw new AssertionError();}catch(IllegalStateException expected){}
   try{new Surface(texture);throw new AssertionError();}catch(IllegalArgumentException expected){}
  }
  Surface window=new Surface();window.widget=123;window.release();check(window.isValid());
  System.out.println("VIDEO_CONSUMER_LIFECYCLE_OK cycles=100 real_jni_queue=1 bounded_callbacks=1 stale_callbacks_cancelled=1 idempotent_release=1");
 }
}
'''
native = r'''
#include <stdint.h>
#include <string.h>
#include "atl_surface_texture.h"
JNIEXPORT jboolean JNICALL Java_SurfaceTextureLifecycleSmoke_offer(JNIEnv *env,jclass ignored,jobject texture){
 jclass type=(*env)->GetObjectClass(env,texture);jmethodID acquire=(*env)->GetMethodID(env,type,"acquireNativeConsumer","()J");
 AtlTextureConsumer *consumer=(void*)(intptr_t)(*env)->CallLongMethod(env,texture,acquire);
 if((*env)->ExceptionCheck(env)){(*env)->DeleteLocalRef(env,type);return JNI_FALSE;}
 AVFrame *frame=av_frame_alloc();frame->format=AV_PIX_FMT_RGBA;frame->width=4;frame->height=4;frame->pts=123;
 if(av_frame_get_buffer(frame,0)<0){atl_texture_consumer_unref(consumer);av_frame_free(&frame);return JNI_FALSE;}
 memset(frame->data[0],0,frame->linesize[0]*frame->height);struct SwsContext *conversion=NULL;
 int ok=atl_texture_consumer_queue(consumer,frame,&conversion);av_frame_free(&frame);sws_freeContext(conversion);
 if(ok){jmethodID method=(*env)->GetMethodID(env,type,"onNativeFrameAvailable","()V");(*env)->CallVoidMethod(env,texture,method);}
 atl_texture_consumer_unref(consumer);(*env)->DeleteLocalRef(env,type);return ok?JNI_TRUE:JNI_FALSE;
}
'''
with tempfile.TemporaryDirectory(prefix='clbb-video-lifecycle-') as temporary:
 directory=pathlib.Path(temporary);files=[]
 for name,text in stubs.items():
  path=directory/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(text);files.append(str(path))
 test=directory/'SurfaceTextureLifecycleSmoke.java';test.write_text(fixture);files.append(str(test))
 files += [str(sources/name) for name in ['android/graphics/SurfaceTexture.java','android/view/Surface.java','android/media/MediaCodec.java']]
 subprocess.run(['javac','--release','8','-Xlint:-options','-d',str(directory),*files],check=True)
 c=directory/'producer.c';c.write_text(native);library=directory/'libconsumer.so'
 backend=root/'src/atl/src/api-impl-jni/graphics'
 flags=shlex.split(subprocess.check_output(['pkg-config','--cflags','--libs','epoxy','glib-2.0','libavcodec','libavutil','libswscale'],text=True))
 subprocess.run(['cc','-shared','-fPIC','-I/usr/lib/jvm/java-21-openjdk/include','-I/usr/lib/jvm/java-21-openjdk/include/linux','-I'+str(backend),str(c),str(backend/'atl_surface_texture.c'),*flags,'-o',str(library)],check=True)
 subprocess.run(['java','-cp',str(directory),'SurfaceTextureLifecycleSmoke',str(library)],check=True)
