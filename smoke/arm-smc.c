#include <jni.h>
#include <sys/mman.h>
#include <stdint.h>
#include <unistd.h>
JNIEXPORT jint JNICALL Java_CLBBArmSmc_check(JNIEnv *env, jclass cls) {
    size_t size = (size_t)sysconf(_SC_PAGESIZE);
    volatile uint32_t *code = mmap(0, size, PROT_READ | PROT_WRITE | PROT_EXEC,
                                   MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (code == MAP_FAILED) return -100;
    code[0] = 0x52800540; // mov w0, #42
    code[1] = 0xd65f03c0; // ret
    int (*function)(void) = (int (*)(void))(uintptr_t)code;
    int result = function();
    if (result == 42) {
        code[0] = 0x52800560; // mov w0, #43; triggers QEMU's translated-page protection
        result = function();
    }
    munmap((void *)code, size);
    return result;
}
