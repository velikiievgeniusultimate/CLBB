#!/usr/bin/env bash
set -euo pipefail
CLBB_TEXTURE_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
CLBB_TEXTURE_TMP="$(mktemp -d "${TMPDIR:-/tmp}/clbb-texture.XXXXXXXX")"
trap 'rm -rf -- "$CLBB_TEXTURE_TMP"' EXIT
ffmpeg -hide_banner -loglevel error -f lavfi -i color=c=red:s=16x16:r=1 -frames:v 1 -c:v libx264 -pix_fmt yuv420p -f h264 "$CLBB_TEXTURE_TMP/red.h264"
cc -Wall -Wextra -Wno-unused-parameter -I/usr/lib/jvm/java-21-openjdk/include -I/usr/lib/jvm/java-21-openjdk/include/linux \
    "$CLBB_TEXTURE_ROOT/smoke/check-surface-texture.c" \
    "$CLBB_TEXTURE_ROOT/src/atl/src/api-impl-jni/graphics/atl_surface_texture.c" \
    $(pkg-config --cflags --libs epoxy glib-2.0 libavcodec libavutil libswscale) -o "$CLBB_TEXTURE_TMP/check"
EGL_PLATFORM=surfaceless LIBGL_ALWAYS_SOFTWARE=1 MESA_GLES_VERSION_OVERRIDE=2.0 "$CLBB_TEXTURE_TMP/check" 2 "$CLBB_TEXTURE_TMP/red.h264"
EGL_PLATFORM=surfaceless LIBGL_ALWAYS_SOFTWARE=1 "$CLBB_TEXTURE_TMP/check" 3 "$CLBB_TEXTURE_TMP/red.h264"
