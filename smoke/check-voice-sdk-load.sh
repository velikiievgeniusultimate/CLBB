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
 public static void main(String[] args) {
  System.load(args[0]);
  System.out.println("CLBB_VOICE_SDK_LOAD_OK=1");
  System.exit(0);
 }
}
JAVA
javac --release 8 -Xlint:-options -d "$work/classes" "$work/CLBBVoiceLoad.java"
dx --dex --output="$work/probe.jar" "$work/classes"
NB_QEMU_TRACE_LOADER=1 NB_QEMU_SYSROOT="$CLBB_ATL_PREFIX/share/libnb-qemu-guest" \
 timeout -k 2s 20s dalvikvm -Xforce-nb-testing -XX:NativeBridge="$CLBB_ATL_PREFIX/lib/libnb-qemu.so" \
 -cp "$work/probe.jar:$CLBB_ATL_PREFIX/lib/java/dex/android_translation_layer/api-impl.jar:$ROOT/game/split_GSDKRTC.apk:$ROOT/game/base.apk" \
 CLBBVoiceLoad "$ROOT/data/base.apk_/lib/libbyteplusaudio.so" > "$work/load.log" 2>&1 || { cat "$work/load.log"; exit 1; }
rg -a 'CLBB_VOICE_SDK_LOAD_OK=' "$work/load.log"
