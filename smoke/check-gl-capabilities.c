#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <GLES3/gl32.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "GLES_bio.h"
/* This fixture uses real EGL surfaces, without ATL's emulated default FBO. */
void bionic_glGetIntegerv(GLenum name, GLint *value) { glGetIntegerv(name, value); }
static void check(int condition, const char *message) {
    if (!condition) { fprintf(stderr, "%s\n", message); exit(1); }
}
int main(void) {
    EGLDisplay display = eglGetPlatformDisplay(EGL_PLATFORM_SURFACELESS_MESA, EGL_DEFAULT_DISPLAY, NULL);
    check(eglInitialize(display, NULL, NULL), "EGL initialize failed");
    EGLint config_attributes[] = {EGL_RENDERABLE_TYPE, EGL_OPENGL_ES3_BIT, EGL_SURFACE_TYPE, EGL_PBUFFER_BIT, EGL_NONE};
    EGLConfig config; EGLint count;
    check(eglChooseConfig(display, config_attributes, &config, 1, &count) && count == 1, "EGL config failed");
    EGLint context_attributes[] = {EGL_CONTEXT_CLIENT_VERSION, 3, EGL_NONE};
    EGLContext contexts[2];
    for (int i=0; i<2; ++i) {
        contexts[i] = eglCreateContext(display, config, EGL_NO_CONTEXT, context_attributes);
        check(contexts[i] != EGL_NO_CONTEXT, "EGL context failed");
    }
    for (int round=0; round<4; ++round) {
        check(eglMakeCurrent(display, EGL_NO_SURFACE, EGL_NO_SURFACE, contexts[round%2]), "EGL make current failed");
        const char *joined = (const char *)nb_glGetString(GL_EXTENSIONS);
        check(joined != NULL, "Extension string missing");
        check(!strstr(joined, "GL_KHR_debug") && !strstr(joined, "GL_EXT_debug_label") && !strstr(joined, "GL_OVR_multiview") && !strstr(joined, "GL_OES_texture_view"), "Unsupported capability leaked");
        GLint filtered_count=-1; nb_glGetIntegerv(GL_NUM_EXTENSIONS, &filtered_count);
        check(filtered_count > 0, "No filtered extensions");
        size_t offset=0;
        for (int i=0; i<filtered_count; ++i) {
            const char *name=(const char *)nb_glGetStringi(GL_EXTENSIONS, i);
            check(name && !strncmp(joined+offset, name, strlen(name)), "Indexed extension differs from legacy string");
            offset += strlen(name);
            check(joined[offset] == (i+1 == filtered_count ? '\0' : ' '), "Extension count mismatch");
            if (joined[offset]) ++offset;
        }
        GLint max_texture=0, actual_max=0;
        nb_glGetIntegerv(GL_MAX_TEXTURE_SIZE, &max_texture); glGetIntegerv(GL_MAX_TEXTURE_SIZE, &actual_max);
        check(max_texture == actual_max && max_texture > 0, "Ordinary query changed");
        check(nb_glGetString(GL_VENDOR) == glGetString(GL_VENDOR), "Vendor query changed");
    }
    eglMakeCurrent(display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT);
    for (int i=0; i<2; ++i) eglDestroyContext(display, contexts[i]);
    eglTerminate(display);
    puts("CLBB_GL_CAPABILITIES_OK=4");
}
