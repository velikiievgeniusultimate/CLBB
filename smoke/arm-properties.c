#define _GNU_SOURCE
#include <jni.h>
#include <dlfcn.h>
#include <stdint.h>
#include <string.h>
#include <sys/system_properties.h>

/* Link this test against guest libc/dl only, never against liblog. */
JNIEXPORT jint JNICALL Java_CLBBArmProperties_check(JNIEnv* env, jclass cls) {
    (void)env; (void)cls;
    void* libc = dlopen("libc.so", RTLD_NOW | RTLD_LOCAL);
    if (!libc) return -1;
    const char* names[] = {
        "__system_property_get", "__system_property_find", "__system_property_read"
    };
    for (unsigned i = 0; i < 3; ++i) {
        void* symbol = dlsym(libc, names[i]);
        Dl_info info;
        if (!symbol || !dladdr(symbol, &info) || !info.dli_fname ||
            !strstr(info.dli_fname, "libc.so")) { dlclose(libc); return -10 - i; }
    }
    const char* queries[] = {"clbb.test.absent", "ro.product.model", "ro.build.version.sdk"};
    for (unsigned i = 0; i < 3; ++i) {
        char value[PROP_VALUE_MAX + 1];
        memset(value, 0x5a, sizeof(value));
        if (__system_property_get(queries[i], value) != 0 || value[0] != '\0') {
            dlclose(libc); return -20 - i;
        }
        for (unsigned j = 1; j < sizeof(value); ++j) {
            if (value[j] != 0x5a) { dlclose(libc); return -30 - i; }
        }
        if (__system_property_find(queries[i]) != NULL) { dlclose(libc); return -40 - i; }
    }
    char name[PROP_NAME_MAX + 1], value[PROP_VALUE_MAX + 1];
    memset(name, 0x5a, sizeof(name));
    memset(value, 0x5a, sizeof(value));
    if (__system_property_read(NULL, name, value) != 0 || name[0] || value[0]) {
        dlclose(libc); return -50;
    }
    for (unsigned i = 1; i < sizeof(name); ++i) {
        if (name[i] != 0x5a) { dlclose(libc); return -51; }
    }
    for (unsigned i = 1; i < sizeof(value); ++i) {
        if (value[i] != 0x5a) { dlclose(libc); return -52; }
    }
    if (__system_property_read(NULL, NULL, NULL) != 0) { dlclose(libc); return -53; }
    dlclose(libc);
    return 3;
}
