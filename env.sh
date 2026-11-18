#!/usr/bin/env bash
CLBB_ATL_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
CLBB_STATE_ROOT="${CLBB_STATE_ROOT:-$CLBB_ATL_ROOT}"
export CLBB_ATL_ROOT CLBB_STATE_ROOT
export CLBB_UPDATE_MANAGER="$CLBB_ATL_ROOT/bin/clbb-manager"
export CLBB_RELEASE_VERSION="$(cat "$CLBB_ATL_ROOT/VERSION" 2>/dev/null || printf dev)"
CLBB_RELEASE_REVISION=unknown
if [[ -f "$CLBB_ATL_ROOT/REVISION" ]]; then IFS= read -r CLBB_RELEASE_REVISION < "$CLBB_ATL_ROOT/REVISION" || true; fi
export CLBB_RELEASE_REVISION
CLBB_ATL_PREFIX="$CLBB_ATL_ROOT/prefix"
export PATH="$CLBB_ATL_PREFIX/bin:$PATH"
export PKG_CONFIG_PATH="$CLBB_ATL_PREFIX/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
# The workbench publishes a key only after verifying all ART libraries and jars.
clbb_art_prefix="$CLBB_ATL_PREFIX"
unset CLBB_ART_CACHE_DIR
if [[ -r "$CLBB_ATL_ROOT/../.clbb-art-key" ]] && IFS= read -r clbb_art_key < "$CLBB_ATL_ROOT/../.clbb-art-key"; then
    if [[ $clbb_art_key =~ ^\"([0-9a-f]{64})\"$ ]]; then
        clbb_candidate_dir="${CLBB_ATL_ROOT%/*}"
        clbb_candidates="${clbb_candidate_dir%/*}"
        clbb_shared_prefix="${clbb_candidates%/*}/art-components/${BASH_REMATCH[1]}/prefix"
        if [[ -r "$clbb_shared_prefix/lib/art/libart.so" ]]; then
            clbb_art_prefix="$clbb_shared_prefix"
            export CLBB_ART_CACHE_DIR="$CLBB_STATE_ROOT/cache/art-${BASH_REMATCH[1]}"
        fi
    fi
fi
export LD_LIBRARY_PATH="$CLBB_ATL_PREFIX/lib:$clbb_art_prefix/lib/art:$clbb_art_prefix/lib/java/dex/art/natives${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export LIBRARY_PATH="$CLBB_ATL_PREFIX/lib:$CLBB_ATL_PREFIX/lib/art${LIBRARY_PATH:+:$LIBRARY_PATH}"
export CPATH="$CLBB_ATL_PREFIX/include${CPATH:+:$CPATH}"
export JAVA_HOME=/usr/lib/jvm/java-21-openjdk

export ANDROID_DATA="$CLBB_STATE_ROOT/art-data"
export XDG_CACHE_HOME="$CLBB_STATE_ROOT/cache"
if [[ ! -d "$ANDROID_DATA" || ! -d "$XDG_CACHE_HOME" ]]; then
    mkdir -p "$ANDROID_DATA" "$XDG_CACHE_HOME"
fi
