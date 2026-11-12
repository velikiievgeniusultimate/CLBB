#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Arch-family source bootstrap. Does not import a game or account data.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || { echo 'Linux x86_64 required' >&2; exit 1; }
command -v pacman >/dev/null || { echo 'Arch-family build dependencies required' >&2; exit 1; }
if [[ ${1:-} == --install-deps ]]; then
  sudo pacman -S --needed -- base-devel clang lld meson ninja git rust cargo pkgconf python \
    jdk21-openjdk asm java-getopt autoconf automake libtool gtk4 webkitgtk-6.0 libportal-gtk4 \
    libglvnd sdl3 sdl2-compat libsndfile capstone ffmpeg libffi gnutls alsa-lib libx11 libxfixes libunwind libbsd libelf libcap icu zlib lz4 xz expat fontconfig patchelf
fi
python3 distribution/bootstrap-sources.py
source ./env.sh
export PATH="$JAVA_HOME/bin:$PATH"
mkdir -p "$CLBB_ATL_PREFIX"/lib/pkgconfig "$CLBB_ATL_PREFIX"/share/java
JOBS=${CLBB_BUILD_JOBS:-4}
cd "$CLBB_ATL_ROOT/src/wolfssl"
[[ -x configure ]] || autoreconf -fi
./configure --prefix="$CLBB_ATL_PREFIX" --enable-shared --disable-opensslall --disable-opensslextra \
  --enable-aescbc-length-checks --enable-curve25519 --enable-ed25519 --enable-ed25519-stream \
  --enable-oldtls --enable-base64encode --enable-tlsx --enable-scrypt --disable-examples \
  --enable-crl --with-rsa --enable-certs --enable-cert-gen --enable-sessioncerts --enable-jni
make -j "$JOBS"
make install
cd "$CLBB_ATL_ROOT"
for component in bionic_translation libglvnd; do
  if [[ -f src/$component/build/meson-private/coredata.dat ]]; then mode=--reconfigure; else mode=; fi
  meson setup ${mode:+$mode} "src/$component/build" "src/$component" --prefix "$CLBB_ATL_PREFIX" --libdir lib --buildtype release
  meson compile -C "src/$component/build" -j "$JOBS"
  meson install -C "src/$component/build" --no-rebuild
done
make -C src/art_standalone -j "$JOBS" ____PREFIX="$CLBB_ATL_PREFIX" ____LIBDIR=lib
make -C src/art_standalone ____PREFIX="$CLBB_ATL_PREFIX" ____LIBDIR=lib install
# Build-only Java dependencies. These do not enter the runtime bundle.
python3 - "$CLBB_ATL_PREFIX/share/java" <<'PY'
from pathlib import Path
import shutil,sys
out=Path(sys.argv[1])
for name,candidates in {'asm.jar':['/usr/share/java/asm/asm.jar','/usr/share/java/asm.jar'], 'asm-tree.jar':['/usr/share/java/asm/asm-tree.jar','/usr/share/java/asm-tree.jar'], 'java-getopt.jar':['/usr/share/java/java-getopt.jar','/usr/share/java/gnu-getopt.jar']}.items():
 source=next((Path(p)for p in candidates if Path(p).is_file()),None)
 if source is None:raise SystemExit('Locate installed build dependency: '+name)
 shutil.copyfile(source,out/name)
PY
bash build-runtime.sh
bash build-arm-zlib.sh
cargo build --manifest-path launcher/Cargo.toml --release --offline --locked
cargo build --manifest-path distribution/manager/Cargo.toml --release --offline --locked
printf '%s\n' 'CLBB runtime built. See distribution/README.md for packaging and game import.'
