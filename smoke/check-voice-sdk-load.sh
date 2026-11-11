#!/usr/bin/env bash
# Real game SDK load/JNI_OnLoad smoke; no game launch, capture or network session.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/env.sh"
[[ -f "$ROOT/data/base.apk_/lib/libbyteplusaudio.so" && -f "$ROOT/game/split_GSDKRTC.apk" ]] || { printf '%s\n' 'SKIP: install game locally first'; exit 77; }
ulimit -c 0
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir "$work/classes"
cat > "$work/CLBBVoiceLoad.java" <<'JAVA'
public final class CLBBVoiceLoad {
 public static void main(String[] args) throws Exception {
  System.load(args[0]);
  Class<?> effects = Class.forName("com.ss.bytertc.audio.device.webrtc.WebRtcAudioEffects");
  for(String name : new String[]{"isAcousticEchoCancelerSupported","isNoiseSuppressorSupported"}) {
   java.lang.reflect.Method method=effects.getDeclaredMethod(name);method.setAccessible(true);
   if(!Boolean.FALSE.equals(method.invoke(null)))throw new AssertionError("Unsupported host effect advertised: "+name);
  }
  android.app.ActivityManager manager=new android.app.ActivityManager();
  android.os.Debug.MemoryInfo[] process=manager.getProcessMemoryInfo(new int[]{android.os.Process.myPid()});
  if(process.length!=1||process[0].getTotalPss()<=0)throw new AssertionError("Missing real process PSS");
  android.app.ActivityManager.MemoryInfo memory=new android.app.ActivityManager.MemoryInfo();manager.getMemoryInfo(memory);
  if(memory.totalMem<=0||memory.availMem<0||memory.availMem>memory.totalMem)throw new AssertionError("Invalid host memory units");
  if(new android.os.BatteryManager().getLongProperty(4)!=99L)throw new AssertionError("Battery capacity");
  if(new android.os.BatteryManager().getLongProperty(123)!=Long.MIN_VALUE)throw new AssertionError("Unsupported battery property");
  Class<?> monitor=Class.forName("com.bytedance.realx.base.RXPerformanceMonitorAndroid",false,CLBBVoiceLoad.class.getClassLoader());
  for(String name:new String[]{"getCurrentPidMemorySize","getJavaAppMemoryUsage","getMemoryState","getThermalState","getThreadCount"}) {
   monitor.getDeclaredMethod(name);
  }
  System.out.println("CLBB_VOICE_SDK_LOAD_OK=1 effects=checked performance_api=checked battery=99 memory=host");
  System.exit(0);
 }
}
JAVA
javac --release 8 -Xlint:-options -cp "$ROOT/src/atl/build/src/api-impl/hax.jar" -d "$work/classes" "$work/CLBBVoiceLoad.java"
dx --dex --output="$work/probe.jar" "$work/classes"
NB_QEMU_TRACE_LOADER=1 NB_QEMU_SYSROOT="$CLBB_ATL_PREFIX/share/libnb-qemu-guest" \
 timeout -k 2s 20s dalvikvm -Xforce-nb-testing -XX:NativeBridge="$CLBB_ATL_PREFIX/lib/libnb-qemu.so" \
 -cp "$work/probe.jar:$CLBB_ATL_PREFIX/lib/java/dex/android_translation_layer/api-impl.jar:$ROOT/game/split_GSDKRTC.apk:$ROOT/game/base.apk" \
 CLBBVoiceLoad "$ROOT/data/base.apk_/lib/libbyteplusaudio.so" > "$work/load.log" 2>&1 || { cat "$work/load.log"; exit 1; }
rg -a 'CLBB_VOICE_SDK_LOAD_OK=' "$work/load.log"
