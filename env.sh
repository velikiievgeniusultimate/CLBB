#!/usr/bin/env bash
CLBB_ATL_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
CLBB_STATE_ROOT="${CLBB_STATE_ROOT:-$CLBB_ATL_ROOT}"
export CLBB_ATL_ROOT CLBB_STATE_ROOT
export CLBB_UPDATE_MANAGER="$CLBB_ATL_ROOT/bin/clbb-manager"
export CLBB_RELEASE_VERSION="$(cat "$CLBB_ATL_ROOT/VERSION" 2>/dev/null || printf dev)"
CLBB_ATL_PREFIX="$CLBB_ATL_ROOT/prefix"
export PATH="$CLBB_ATL_PREFIX/bin:$PATH"
export PKG_CONFIG_PATH="$CLBB_ATL_PREFIX/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export LD_LIBRARY_PATH="$CLBB_ATL_PREFIX/lib:$CLBB_ATL_PREFIX/lib/art:$CLBB_ATL_PREFIX/lib/java/dex/art/natives${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export LIBRARY_PATH="$CLBB_ATL_PREFIX/lib:$CLBB_ATL_PREFIX/lib/art${LIBRARY_PATH:+:$LIBRARY_PATH}"
export CPATH="$CLBB_ATL_PREFIX/include${CPATH:+:$CPATH}"
export JAVA_HOME=/usr/lib/jvm/java-21-openjdk

export ANDROID_DATA="$CLBB_STATE_ROOT/art-data"
mkdir -p "$ANDROID_DATA"

export XDG_CACHE_HOME="$CLBB_STATE_ROOT/cache"
mkdir -p "$XDG_CACHE_HOME"
