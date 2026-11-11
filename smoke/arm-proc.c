#include <jni.h>
#include <dlfcn.h>
#include <string.h>
JNIEXPORT jint JNICALL Java_CLBBArmProc_check(JNIEnv *env, jclass cls) {
    void *egl = dlopen("libEGL.so", RTLD_NOW | RTLD_LOCAL);
    void *gles = dlopen("libGLESv2.so", RTLD_NOW | RTLD_LOCAL);
    if (!egl || !gles) return -1;
    void *(*get_proc)(const char *) = dlsym(egl, "eglGetProcAddress");
    if (!get_proc) return -2;
    const char *names[] = {"glTexStorage2DMultisample", "glTexBuffer", "glTexBufferEXT", "glBindFramebuffer", "glGetStringi"};
    for (int i=0; i<5; ++i) {
        void *entry = get_proc(names[i]);
        if (!entry || entry != dlsym(gles,names[i])) return -10-i;
    }
    if (get_proc(NULL) || get_proc("glDebugMessageCallback") || get_proc("glDebugMessageCallbackKHR")) return -20;
    dlclose(gles); dlclose(egl);
    return 5;
}
