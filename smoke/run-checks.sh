#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/env.sh"
ulimit -c 0
for CHECK in Threads Args Signal Attach Varargs Smc SignalReturn SignalNested Proc Yield AttachExit Dns LoadFailure Properties; do
    case "$CHECK" in
        Threads) LIB=smoke;;
        Args) LIB=args;;
        Signal) LIB=signal;;
        Attach) LIB=attach;;
        Varargs) LIB=varargs;;
        Smc) LIB=smc;;
        SignalReturn) LIB=signal-return;;
        SignalNested) LIB=signal-nested;;
        Proc) LIB=proc;;
        Yield) LIB=yield;;
        AttachExit) LIB=attach-exit;;
        Dns) LIB=dns;;
        LoadFailure) LIB=load-failure;;
        Properties) LIB=properties;;
    esac
    LOG="$ROOT/logs/check-$CHECK.log"
    NB_QEMU_SYSROOT="$CLBB_ATL_PREFIX/share/libnb-qemu-guest" \
      timeout -k 3s 25s dalvikvm -Xforce-nb-testing \
      -XX:NativeBridge="$CLBB_ATL_PREFIX/lib/libnb-qemu.so" \
      -cp "$ROOT/smoke/smoke.jar" "CLBBArm$CHECK" \
      "$ROOT/smoke/libclbb-arm-$LIB.so" > "$LOG" 2>&1
    rg -a 'CLBB_.*OK=' "$LOG"
done
python "$ROOT/smoke/check-return-width.py"
python3 "$ROOT/smoke/check-generated-stack.py"
python3 "$ROOT/smoke/check-framebuffer-binding.py"
cc -g -fsanitize=address,undefined -I "$ROOT/src/nb-qemu/libnb-qemu" \
  $(pkg-config --cflags glib-2.0) "$ROOT/smoke/check-gl-capabilities.c" \
  "$ROOT/src/nb-qemu/libnb-qemu/libGLESv2.itf.c" \
  -o "$ROOT/smoke/check-gl-capabilities" $(pkg-config --libs glib-2.0 egl glesv2)
ASAN_OPTIONS=detect_leaks=0 "$ROOT/smoke/check-gl-capabilities"
cc -g -O1 -pthread "$ROOT/smoke/check-rcu-loader.c" -ldl -o "$ROOT/smoke/check-rcu-loader"
timeout -k 2s 15s "$ROOT/smoke/check-rcu-loader" "$CLBB_ATL_PREFIX/lib/libnb-qemu.so"
bash "$ROOT/dns-adapter/test.sh"
cargo test --offline --locked --manifest-path "$ROOT/mapper/core/Cargo.toml"
cargo test --offline --locked --manifest-path "$ROOT/mapper/runtime/Cargo.toml"
"$ROOT/tools/rsqrt-estimate-audit/check.sh"
python3 "$ROOT/smoke/check-package-permissions.py"
python3 "$ROOT/smoke/check-google-package-presence.py"
python3 "$ROOT/smoke/check-startup-native-trace.py"
"$ROOT/mapper/ui/check-viewport.sh"
"$ROOT/mapper/ui/check-preview.sh"
python3 "$ROOT/smoke/check-mapper-host.py"
python3 "$ROOT/smoke/check-mapper-confine.py"
python3 "$ROOT/smoke/check-motion-event.py"
python3 "$ROOT/smoke/check-frame-pacing.py"
python3 "$ROOT/smoke/check-resource-only-dex.py"
python3 "$ROOT/smoke/check-tcg-cache-size.py"
python3 "$ROOT/tools/check-fd-trans.py"
"$ROOT/tools/tb-state-audit/check.sh"
bash "$ROOT/tools/pauth-audit/check-guest-simd.sh"
bash "$ROOT/smoke/check-libc-contract.sh" --run
python3 "$ROOT/tools/check-guest-build-contract.py"
python3 "$ROOT/tools/check-bionic-build-contract.py"
bash "$ROOT/smoke/check-fstatat-contract.sh" --run

bash "$ROOT/execution-core/check.sh"

# Retired ETA-only checks are stored in the verified external ETA archive.
