#!/usr/bin/env python3
"""Expose already extracted native assets to the experimental loader unchanged."""
import os
import sys
from pathlib import Path


def prepare(root: Path) -> None:
    sys.path.insert(0,str(Path(__file__).resolve().parent / "distribution"))
    from native_libs import prepare as prepare_libraries
    print("CLBB_NATIVE_LIBRARIES=" + str(prepare_libraries(root,locked=os.environ.get("CLBB_NATIVE_LOCK_HELD")=="1")),flush=True)
    payload_dir = root / "data/base.apk_/files/dragon2017/assets/comlibs/arm64-v8a"
    alias_dir = root / "data/native-plugin-aliases"
    alias_dir.mkdir(parents=True, exist_ok=True)
    expected = {}
    for payload in sorted(payload_dir.glob("lib*.bytes")):
        with payload.open("rb") as source:
            header = source.read(20)
        # ELF64, little endian, EM_AARCH64. Compressed/non-native assets stay out.
        if len(header) == 20 and header[:6] == b"\x7fELF\x02\x01" and int.from_bytes(header[18:20], "little") == 183:
            expected[payload.stem + ".so"] = payload.resolve()
    # This directory belongs only to this staging helper. Preserve regular files
    # rather than overwriting an unexpected user artifact.
    for alias in alias_dir.glob("lib*.so"):
        if alias.is_symlink() and alias.name not in expected:
            alias.unlink()
    for name, payload in expected.items():
        alias = alias_dir / name
        if alias.is_symlink():
            if alias.resolve() == payload:
                continue
            alias.unlink()
        elif alias.exists():
            raise RuntimeError(f"Refusing to overwrite a regular file: {alias}")
        alias.symlink_to(payload)


if __name__ == "__main__":
    prepare(Path(os.environ.get("CLBB_STATE_ROOT", Path(__file__).resolve().parent)))
