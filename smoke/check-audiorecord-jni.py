#!/usr/bin/env python3
"""Exercise production Java/JNI with deterministic PCM, including failed initialization."""
from pathlib import Path
import os, subprocess, tempfile
ROOT=Path(__file__).resolve().parents[1]
def run(args,**kw):subprocess.run(list(map(str,args)),check=True,**kw)
with tempfile.TemporaryDirectory(prefix='clbb-record-jni-') as tmp:
    tmp=Path(tmp); package=tmp/'android/media';package.mkdir(parents=True)
    (package/'AudioRecord.java').write_bytes((ROOT/'src/atl/src/api-impl/android/media/AudioRecord.java').read_bytes())
    (package/'AudioFormat.java').write_text('package android.media; public class AudioFormat {public int sampleRate=48000,channelMask=16,encoding=2;}')
    (tmp/'Test.java').write_text('''import android.media.AudioRecord; import java.nio.ByteBuffer;
public class Test {
 static void check(boolean value){if(!value)throw new AssertionError();}
 public static void main(String[] args){System.load(args[0]);
  check(AudioRecord.getMinBufferSize(48000,16,2)==1920);
  check(AudioRecord.getMinBufferSize(48000,123,2)==-2);
  AudioRecord invalid=new AudioRecord(1,48000,123,2,1920);check(invalid.getState()==0);check(invalid.read(new short[10],0,10)==-3);invalid.release();
  AudioRecord a=new AudioRecord(1,48000,16,2,1920);check(a.getState()==1);a.startRecording();check(a.getRecordingState()==3);
  short[] s=new short[12];check(a.read(s,2,6)==6);check(s[0]==0&&s[2]==0x1234&&s[7]==0x1234&&s[8]==0);check(a.read(s,Integer.MAX_VALUE,10)==-2);
  byte[] b=new byte[14];check(a.read(b,2,8)==8);check(b[2]==0x34&&b[3]==0x12&&b[10]==0);
  ByteBuffer direct=ByteBuffer.allocateDirect(10);direct.position(3);check(a.read(direct,8)==8);check(direct.position()==3&&direct.get(0)==0x34&&direct.get(1)==0x12);check(a.read(ByteBuffer.allocate(10),8)==-2);
  check(a.read(s,0,6,1)==6);check(a.read(s,0,6,2)==-2);a.stop();check(a.read(s,0,6)==-3);a.startRecording();check(a.read(s,0,6)==6);a.release();a.release();check(a.getState()==0);check(a.getRecordingState()==1);
  System.out.println("AUDIORECORD_JNI_OK PCM16 short/byte/direct, modes, bounds, stop/restart/release, failure state");
 }
}''')
    run(['javac','-h',tmp,'-d',tmp,package/'AudioRecord.java',package/'AudioFormat.java',tmp/'Test.java'])
    java_home=Path(subprocess.check_output(['readlink','-f',subprocess.check_output(['which','javac'],text=True).strip()],text=True).strip()).parents[1]
    source=(ROOT/'src/atl/src/api-impl-jni/audio/android_media_AudioRecord.c').read_text().replace('../defines.h',str(ROOT/'src/atl/src/api-impl-jni/defines.h')).replace('../generated_headers/android_media_AudioRecord.h',str(tmp/'android_media_AudioRecord.h'))
    (tmp/'record.c').write_text(source)
    (tmp/'mock.c').write_text('''#include <stdint.h>
#include <jni.h>
uint64_t clbb_capture_open(uint32_t rate,uint32_t channels){return rate==48000&&channels==1?1:0;}
int clbb_capture_active(uint64_t id,int active){return id==1?0:-3;}
int clbb_capture_read(uint64_t id,int16_t* data,uint32_t frames,int blocking){for(uint32_t i=0;i<frames;i++)data[i]=0x1234;return frames;}
void clbb_capture_close(uint64_t id){}
JNIEXPORT jint JNI_OnLoad(JavaVM *vm,void *reserved){return JNI_VERSION_1_6;}
''')
    run(['cc','-shared','-fPIC','-I'+str(java_home/'include'),'-I'+str(java_home/'include/linux'),tmp/'record.c',tmp/'mock.c','-ldl','-Wl,-z,global','-o',tmp/'record.so'])
    # JVM loads JNI libraries RTLD_LOCAL; preload makes mock capture symbols visible to dlsym.
    run(['java','-cp',tmp,'Test',tmp/'record.so'],env={**os.environ,'LD_PRELOAD':str(tmp/'record.so')})
