#!/usr/bin/env python3
"""Check that the native bridge returns every guest page it allocates."""

import os
from pathlib import Path
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "src/nb-qemu/libnb-qemu/QemuAndroid.c"
ADAPTER = r"""
#include <assert.h>
#include <errno.h>
#include <stdint.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

typedef GUEST_WORD abi_ulong;
static void *last_mapping;
static unsigned maps, unmaps;
static int fail_map, fail_write;

static intptr_t target_mmap(intptr_t start, size_t len, int prot, int flags, int fd, int off) {
    ++maps;
    last_mapping = fail_map ? MAP_FAILED : mmap((void *)start, len, prot, flags, fd, off);
    return (intptr_t)last_mapping;
}
static int target_munmap(intptr_t address, size_t len) {
    ++unmaps;
    return munmap((void *)address, len);
}
static int copy_to_user(intptr_t address, const void *source, size_t len) {
    if (fail_write) return -1;
    memcpy((void *)address, source, len);
    return 0;
}
static int copy_from_user(void *dest, intptr_t address, size_t len) {
    memcpy(dest, (void *)address, len);
    return 0;
}
"""
CASES = r"""
static void check_unmapped(void *page) {
    unsigned char resident;
    assert(mincore(page, 1, &resident) == -1 && errno == ENOMEM);
}
int main(void) {
    const size_t page = (size_t)sysconf(_SC_PAGESIZE);
    const size_t sizes[] = {0, 1, page - sizeof(size_t), page - 1, page, page + 1, 2 * page};
    for (size_t i = 0; i < sizeof(sizes) / sizeof(*sizes); ++i) {
        size_t size = sizes[i];
        intptr_t payload = qemu_android_malloc(size);
        assert(payload && (void *)(payload - sizeof(size_t)) == last_mapping);
        memset((void *)payload, 0x5a, size);
        void *last_page = (char *)last_mapping + ((size + sizeof(size_t) - 1) / page) * page;
        qemu_android_free(payload);
        check_unmapped(last_mapping);
        check_unmapped(last_page);
    }
    unsigned releases = unmaps;
    qemu_android_free(0);
    assert(unmaps == releases);
    fail_map = 1;
    assert(qemu_android_malloc(page) == 0 && unmaps == releases);
    fail_map = 0;
    fail_write = 1;
    assert(qemu_android_malloc(page) == 0 && unmaps == releases + 1);
    check_unmapped((char *)last_mapping + page);
    unsigned allocations = maps;
    assert(qemu_android_malloc(SIZE_MAX) == 0 && maps == allocations);
    if (sizeof(abi_ulong) < sizeof(size_t)) {
        assert(qemu_android_malloc(UINT32_MAX) == 0 && maps == allocations);
    }
}
"""


def main():
    production = SOURCE.read_text()
    start = production.index("intptr_t qemu_android_malloc(")
    end = production.index("void qemu_android_memcpy(", start)
    with tempfile.TemporaryDirectory(prefix="clbb-bridge-allocation-") as temp:
        src = Path(temp) / "check.c"
        src.write_text(ADAPTER + production[start:end] + CASES)
        binary = Path(temp) / "check"
        for word in ("uint32_t", "uint64_t"):
            subprocess.run([os.environ.get("CC", "cc"), "-std=gnu11", "-O2", "-Wall",
                            "-Wextra", "-Werror", "-DGUEST_WORD=" + word,
                            str(src), "-o", str(binary)], check=True)
            subprocess.run([str(binary)], check=True)
    print("Native bridge allocation and release passed for both guest word sizes")


if __name__ == "__main__":
    main()
