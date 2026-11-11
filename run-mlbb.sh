#!/usr/bin/env bash
set -euo pipefail
CLBB_LAUNCH_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
source "$CLBB_LAUNCH_ROOT/env.sh"
# Local VPN manager has a DIRECT application rule for this runtime prefix.
# Explicit inherited proxies would override that process routing rule.
unset HTTP_PROXY HTTPS_PROXY ALL_PROXY http_proxy https_proxy all_proxy
export ANDROID_APP_DATA_DIR="$CLBB_STATE_ROOT/data"
export ATL_UGLY_ENABLE_WEBVIEW=1
export ATL_WEBKIT_EXTENSIONS_DIR="$CLBB_ATL_PREFIX/lib/atl-web-extensions"
export CLBB_WEBKIT_PROCESS_DIR="${CLBB_WEBKIT_PROCESS_DIR:-$CLBB_ATL_PREFIX/lib/webkitgtk-6.0}"
if [[ ! -d "$CLBB_WEBKIT_PROCESS_DIR" ]]; then
    export CLBB_WEBKIT_PROCESS_DIR=/usr/lib/webkitgtk-6.0
fi
export WEBKIT_INJECTED_BUNDLE_PATH="$CLBB_WEBKIT_PROCESS_DIR/injected-bundle"
export LD_PRELOAD="$CLBB_ATL_PREFIX/lib/libclbb_webkit_launch_adapter.so${LD_PRELOAD:+:$LD_PRELOAD}"
# Use the desktop mixer for independent game/video streams when it is available.
if [[ -z "${ATL_AUDIO_DEVICE:-}" && -S "${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/pipewire-0" ]]; then
    export ATL_AUDIO_DEVICE=pipewire
fi
# Both audio backends identify themselves as this one application in desktop mixers.
export SDL_APP_NAME=CLBB
export SDL_AUDIO_DEVICE_APP_NAME=CLBB
export SDL_AUDIO_DEVICE_STREAM_NAME="Mobile Legends"
export PULSE_PROP="${PULSE_PROP:+$PULSE_PROP }application.name=CLBB application.id=org.clbb.CLBB application.icon_name=applications-games"
if [[ -z "${PIPEWIRE_ALSA:-}" ]]; then
    export PIPEWIRE_ALSA='{ application.name = "CLBB" application.id = "org.clbb.CLBB" node.description = "CLBB" media.name = "Mobile Legends" }'
fi
export ATL_SKIP_NATIVES_EXTRACTION=1
export ATL_START_WINDOWED=1
export CLBB_FAST_WINDOW_CLOSE=1
unset ATL_FORCE_FULLSCREEN
export CLBB_ANDROID_ID="$(python3 "$CLBB_LAUNCH_ROOT/distribution/device_identity.py" "$CLBB_STATE_ROOT")"
python3 "$CLBB_LAUNCH_ROOT/prepare-runtime.py" "$CLBB_LAUNCH_ROOT"
export ATL_SYSTEM_FONTS_DIR="$CLBB_STATE_ROOT/system/fonts"
export ATL_SYSTEM_FONTS_CONFIG="$CLBB_STATE_ROOT/system/etc/fonts.xml"
export NB_QEMU_LIBRARY_PATH="$CLBB_STATE_ROOT/data/base.apk_/lib:$CLBB_STATE_ROOT/data/base.apk_/files/libs:$CLBB_STATE_ROOT/data/native-plugin-aliases${NB_QEMU_EXTRA_LIBRARY_PATH:+:$NB_QEMU_EXTRA_LIBRARY_PATH}"
export NB_QEMU_SYSROOT="$CLBB_STATE_ROOT/guest-root"
# Keep more translated game code available between loading stages.
export CLBB_TCG_CACHE_MIB=512
mkdir -p "$ANDROID_APP_DATA_DIR"
CLBB_APK_PARTS=("$CLBB_STATE_ROOT/game/base.apk")
for CLBB_APK_PART in "$CLBB_STATE_ROOT"/game/split_*.apk; do
    [[ -f "$CLBB_APK_PART" ]] && CLBB_APK_PARTS+=("$CLBB_APK_PART")
done
exec "$CLBB_ATL_PREFIX/bin/android-translation-layer" \
    -w 1280 -h 720 \
    -X '-Xforce-nb-testing' \
    -X "-XX:NativeBridge=$CLBB_ATL_PREFIX/lib/libnb-qemu.so" \
    -X "-Datl.app.library.path=$CLBB_ATL_PREFIX/lib/java/dex/android_translation_layer/natives:$NB_QEMU_LIBRARY_PATH" \
    "$@" "${CLBB_APK_PARTS[@]}"
