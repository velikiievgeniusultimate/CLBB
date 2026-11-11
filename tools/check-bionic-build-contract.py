#!/usr/bin/env python3
"""Exercise the real Bionic Makefile with tiny temporary C/C++/assembly inputs.

No production source/builddir/prefix mutation, runtime execution, or game access.
Uses real AArch64 clang compilation, but never links or builds actual libc.
"""
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
MAKEFILE = ROOT / "src/nb-qemu/libnb-qemu-guest/bionic/Makefile"


def main():
    with tempfile.TemporaryDirectory(prefix="clbb-bionic-make-contract-") as tmp:
        base = Path(tmp)
        tree, out = base / "sources", base / "objects"
        tree.mkdir()
        categories = {
            "LIBC_SRCS": "core.c core.cpp",
            "LIBC_SRCS_arm64": "asm.S",
            "LIBC_NOSP_SRCS": "nosp.c",
            "LIBC_FORTIFY_SRC": "fortify.c",
            "LIBC_FREEBSD_SRCS": "freebsd.c",
            "LIBC_NETBSD_SRCS": "netbsd.c",
            "LIBC_OPENBSD_SRCS": "openbsd.c",
            "JEMALLOC_SRCS": "jemalloc.c",
        }
        sources = [name for names in categories.values() for name in names.split()]
        for include in ("libc/kernel/uapi/linux/libc-compat.h",
                        "libc/upstream-freebsd/android/include/freebsd-compat.h",
                        "libc/upstream-netbsd/android/include/netbsd-compat.h",
                        "libc/upstream-openbsd/android/include/openbsd-compat.h",
                        "jemalloc/android/include/log.h", "libc/libc.arm64.map"):
            path = tree / include
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("\n")
        (tree / "deep.h").write_text("#define CONTRACT_VALUE 42\n")
        (tree / "common.h").write_text('#include "deep.h"\n')
        (tree / "only-core-c.h").write_text("/* selective prerequisite */\n")
        for name in sources:
            path = tree / name
            if path.suffix == ".S":
                path.write_text('#include "common.h"\n.text\n.globl contract_asm\ncontract_asm:\nret\n')
            else:
                extra = '#include "only-core-c.h"\n' if name == "core.c" else ""
                path.write_text('#include "common.h"\n' + extra +
                                'int contract_' + name.replace(".", "_") + '(void) { return CONTRACT_VALUE; }\n')
        objects = [out / (name + ".o") for name in sources]
        stamp = out / ".bionic-build-flags"
        common = ["make", "--no-print-directory", "-f", str(MAKEFILE), "-C", str(tree),
                  "CURRDIR=" + str(tree), "BUILDDIR=" + str(out),
                  "INCLUDES=-I" + str(tree),
                  "CFLAGS=-target aarch64-pc-linux -nostdinc -ffreestanding $(BIONIC_OPT)",
                  *[key + "=" + value for key, value in categories.items()]]
        checks = 0

        def run(flags="", dns=1, dry_default=False):
            command = common + ["BIONIC_OPT=" + flags, "CLBB_NATIVE_BRIDGE_DNS=" + str(dns)]
            command += ["-n"] if dry_default else list(map(str, objects))
            result = subprocess.run(command, text=True, capture_output=True)
            if result.returncode:
                raise AssertionError(result.stdout + result.stderr)
            return result.stdout

        def state():
            return [p.stat().st_mtime_ns for p in objects] + [stamp.stat().st_mtime_ns]

        def require(condition, reason):
            nonlocal checks
            if not condition:
                raise AssertionError(reason)
            checks += 1

        output = run()
        require(all(p.exists() and Path(str(p) + ".d").exists() for p in objects),
                "all eight recipe categories must produce dependency files")
        require(all(p.read_bytes()[18:20] == b"\xb7\x00" for p in objects),
                "fixture objects must really be AArch64")
        initial = state()
        run()
        require(state() == initial, "identical flags must preserve objects and stamp")
        release = "-O2 -fno-strict-aliasing -fno-builtin -mno-outline-atomics"
        run(release)
        optimized = state()
        require(all(a != b for a, b in zip(initial, optimized)),
                "flag transition must rebuild every object category")
        run(release)
        require(state() == optimized, "identical release rebuild must be a no-op")
        (tree / "deep.h").write_text("#define CONTRACT_VALUE 43\n")
        run(release)
        changed_header = state()
        require(all(a != b for a, b in zip(optimized[:-1], changed_header[:-1])),
                "transitive header must rebuild C/C++/preprocessed assembly recipes")
        require(optimized[-1] == changed_header[-1], "header changes must not touch flag stamp")
        (tree / "only-core-c.h").write_text("/* selective prerequisite changed */\n")
        run(release)
        selective = state()
        require(selective[0] != changed_header[0] and selective[1:] == changed_header[1:],
                "private header must rebuild only its dependent translation unit")
        # -MP prevents old .d files referring to a removed transitive header from
        # failing before the changed direct include can regenerate dependencies.
        (tree / "common.h").write_text("#define CONTRACT_VALUE 44\n")
        (tree / "deep.h").unlink()
        run(release)
        require(all("deep.h" not in Path(str(p) + ".d").read_text() for p in objects),
                "removed header dependencies must be regenerated without make failure")
        before = state()
        run(release, dns=0)
        after = state()
        require(all(a != b for a, b in zip(before, after)), "DNS define transition must invalidate all objects")
        run("", dns=0)
        require(all(a != b for a, b in zip(after, state())), "rollback to historical flags must invalidate all objects")
        require("-soname,libc.so" in run(dns=0, dry_default=True),
                "default goal must remain libc, not the flag stamp")
        text = stamp.read_text()
        require("CLANG=" in text and "LIBGCC=" in text and "INCLUDES=" in text,
                "stamp must include compiler, support library and include configuration")
        print("BIONIC_BUILD_CONTRACT_CHECKS=" + str(checks))
        print("BIONIC_BUILD_CONTRACT_OK=1")


if __name__ == "__main__":
    main()
