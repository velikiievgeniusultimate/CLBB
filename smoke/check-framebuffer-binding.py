#!/usr/bin/env python3
"""Verify production JNI/ARM framebuffer queries against real Mesa ES2/ES3."""
import os
import pathlib
import re
import shlex
import shutil
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
ATL = ROOT/'src/atl/src'
NB = ROOT/'src/nb-qemu/libnb-qemu'


def extract(source, name):
    tokens = r'"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'|/\*.*?\*/|//[^\n]*'
    masked = re.sub(tokens, lambda match: ' ' * len(match.group()), source, flags=re.S)
    pattern = r'^(?:static\s+)?(?:JNIEXPORT\s+\w+\s+JNICALL|void)\s+' + re.escape(name) + r'\s*\([^;{}]*\)\s*\{'
    match = re.search(pattern, masked, re.M)
    if not match:
        raise RuntimeError('Production function not found: ' + name)
    depth, end = 1, match.end()
    while depth and end < len(masked):
        depth += (masked[end] == '{') - (masked[end] == '}')
        end += 1
    if depth:
        raise RuntimeError('Unbalanced production function: ' + name)
    return source[match.start():end]


ADAPTER = r'''
#define _GNU_SOURCE
#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <GLES3/gl3.h>
#include <glib.h>
#include <jni.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Only the EGL surface lookup is adapted. The context, FBOs and GLES calls
 * are real, and the compiled binding/query function bodies are unchanged. */
struct test_buffer { GLuint gl_framebuffer; };
typedef struct { struct test_buffer *back_buffer; } ATLSurface;
static GHashTable *draw_surface_hashtable;
typedef struct { jint values[6]; } IntArray;
static unsigned mapped, released;
static void check(int condition, const char *message) {
    if (!condition) { fprintf(stderr, "FRAMEBUFFER_TEST_FAILURE: %s\n", message); exit(1); }
}
static jint *get_array(JNIEnv *env, jintArray reference, jboolean *is_copy) {
    (void)env; ++mapped;
    if (is_copy) *is_copy = JNI_FALSE;
    return ((IntArray *)reference)->values;
}
static void release_array(JNIEnv *env, jintArray reference, jint *values, jint mode) {
    (void)env; ++released;
    check(values == ((IntArray *)reference)->values && mode == 0, "JNI query array release");
}
static const struct JNINativeInterface_ jni = {
    .GetIntArrayElements = get_array, .ReleaseIntArrayElements = release_array
};
'''

FIXTURE = r'''
static void query(GLenum name, GLint expected) {
    GLint arm = -1;
    nb_glGetIntegerv(name, &arm);
    check(arm == expected, "ARM query leaked/misidentified a framebuffer");
    JNIEnv env = &jni;
    IntArray array = {.values = {123, 456, -1, 789, 987, 654}};
    Java_android_opengl_GLES20_glGetIntegerv__I_3II(&env, NULL, name, (jintArray)&array, 2);
    check(array.values[2] == expected, "JNI query leaked/misidentified a framebuffer");
    check(array.values[0] == 123 && array.values[1] == 456 && array.values[3] == 789 &&
          array.values[4] == 987 && array.values[5] == 654, "JNI query damaged offset sentinels");
    check(mapped == released, "JNI array mapping leaked");
    check(glGetError() == GL_NO_ERROR, "binding query introduced a GL error");
}
static void run_context(EGLDisplay display, EGLConfig config, EGLSurface pbuffer, int version) {
    EGLint attributes[] = {EGL_CONTEXT_CLIENT_VERSION, version, EGL_NONE};
    EGLContext context = eglCreateContext(display, config, EGL_NO_CONTEXT, attributes);
    check(context != EGL_NO_CONTEXT, "ES context unavailable");
    check(eglMakeCurrent(display, pbuffer, pbuffer, context), "make current");
    const char *gl_version = (const char *)glGetString(GL_VERSION);
    int es3 = gl_version && (strstr(gl_version, "OpenGL ES 3.") != NULL);
    if (version == 3) check(es3, "ES3 context was not provided");
    GLuint names[4], textures[4]; glGenFramebuffers(4, names); glGenTextures(4, textures);
    for (int i = 0; i < 4; ++i) {
        glBindTexture(GL_TEXTURE_2D, textures[i]);
        glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA, 1, 1, 0, GL_RGBA, GL_UNSIGNED_BYTE, NULL);
        glBindFramebuffer(GL_FRAMEBUFFER, names[i]);
        glFramebufferTexture2D(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, textures[i], 0);
        check(glCheckFramebufferStatus(GL_FRAMEBUFFER) == GL_FRAMEBUFFER_COMPLETE, "fixture complete FBO");
    }
    struct test_buffer first = {.gl_framebuffer = names[0]};
    struct test_buffer second = {.gl_framebuffer = names[1]};
    ATLSurface surface = {.back_buffer = &first};
    draw_surface_hashtable = g_hash_table_new(NULL, NULL);
    g_hash_table_insert(draw_surface_hashtable, context, &surface);

    bionic_glBindFramebuffer(GL_FRAMEBUFFER, 0);
    GLint physical = 0; glGetIntegerv(GL_FRAMEBUFFER_BINDING, &physical);
    check((GLuint)physical == first.gl_framebuffer && physical != 0, "fixture default FBO mapping");
    query(GL_FRAMEBUFFER_BINDING, 0);
    bionic_glBindFramebuffer(GL_FRAMEBUFFER, names[2]);
    query(GL_FRAMEBUFFER_BINDING, (GLint)names[2]);
    bionic_glBindFramebuffer(GL_FRAMEBUFFER, 0);

    /* Simulate the runtime selecting its next back buffer during swap. A saved
     * logical zero must bind/query that new buffer rather than expose its ID. */
    rotate_default_framebuffer(first.gl_framebuffer, second.gl_framebuffer);
    surface.back_buffer = &second;
    glGetIntegerv(GL_FRAMEBUFFER_BINDING, &physical);
    check((GLuint)physical == second.gl_framebuffer, "back-buffer rotation mapping");
    query(GL_FRAMEBUFFER_BINDING, 0);
    if (es3) {
        bionic_glBindFramebuffer(GL_READ_FRAMEBUFFER, names[3]);
        rotate_default_framebuffer(second.gl_framebuffer, first.gl_framebuffer);
        surface.back_buffer = &first;
        query(GL_DRAW_FRAMEBUFFER_BINDING, 0);
        query(GL_READ_FRAMEBUFFER_BINDING, (GLint)names[3]);
        bionic_glBindFramebuffer(GL_DRAW_FRAMEBUFFER, names[2]);
        bionic_glBindFramebuffer(GL_READ_FRAMEBUFFER, 0);
        rotate_default_framebuffer(first.gl_framebuffer, second.gl_framebuffer);
        surface.back_buffer = &second;
        query(GL_DRAW_FRAMEBUFFER_BINDING, (GLint)names[2]);
        query(GL_READ_FRAMEBUFFER_BINDING, 0);
        bionic_glBindFramebuffer(GL_READ_FRAMEBUFFER, names[3]);
        rotate_default_framebuffer(second.gl_framebuffer, first.gl_framebuffer);
        surface.back_buffer = &first;
        query(GL_DRAW_FRAMEBUFFER_BINDING, (GLint)names[2]);
        query(GL_READ_FRAMEBUFFER_BINDING, (GLint)names[3]);
        /* The scene resolve must still read the user's scene after present,
         * rather than feed the last displayed frame back into postprocessing. */
        bionic_glBindFramebuffer(GL_DRAW_FRAMEBUFFER, names[3]);
        glClearColor(1, 0, 0, 1); glClear(GL_COLOR_BUFFER_BIT);
        bionic_glBindFramebuffer(GL_DRAW_FRAMEBUFFER, 0);
        glClearColor(0, 0, 1, 1); glClear(GL_COLOR_BUFFER_BIT);
        rotate_default_framebuffer(first.gl_framebuffer, second.gl_framebuffer);
        surface.back_buffer = &second;
        bionic_glBindFramebuffer(GL_DRAW_FRAMEBUFFER, names[2]);
        glBlitFramebuffer(0, 0, 1, 1, 0, 0, 1, 1, GL_COLOR_BUFFER_BIT, GL_NEAREST);
        bionic_glBindFramebuffer(GL_READ_FRAMEBUFFER, names[2]);
        unsigned char pixel[4]; glReadPixels(0, 0, 1, 1, GL_RGBA, GL_UNSIGNED_BYTE, pixel);
        check(pixel[0] == 255 && pixel[1] == 0 && pixel[2] == 0 && pixel[3] == 255,
              "present redirected scene resolve into a displayed frame");
        bionic_glBindFramebuffer(GL_FRAMEBUFFER, 0);
        query(GL_DRAW_FRAMEBUFFER_BINDING, 0);
        query(GL_READ_FRAMEBUFFER_BINDING, 0);
        const GLenum back = GL_BACK;
        const GLenum attachments[] = {GL_COLOR, GL_DEPTH, GL_STENCIL};
        nb_glReadBuffer(GL_BACK);
        nb_glDrawBuffers(1, &back);
        check(glGetError() == GL_NO_ERROR, "default read/draw buffer enums were not translated");
        GLint selected;
        glGetIntegerv(GL_READ_BUFFER, &selected);
        check(selected == GL_COLOR_ATTACHMENT0, "physical default read attachment");
        glGetIntegerv(GL_DRAW_BUFFER0, &selected);
        check(selected == GL_COLOR_ATTACHMENT0, "physical default draw attachment");
        nb_glInvalidateFramebuffer(GL_FRAMEBUFFER, 3, attachments);
        nb_glInvalidateSubFramebuffer(GL_READ_FRAMEBUFFER, 3, attachments, 0, 0, 1, 1);
        check(glGetError() == GL_NO_ERROR, "default invalidation enums were not translated");
        check(attachments[0] == GL_COLOR && attachments[1] == GL_DEPTH && attachments[2] == GL_STENCIL,
              "invalidation modified caller array");
        bionic_glBindFramebuffer(GL_FRAMEBUFFER, names[2]);
        glReadBuffer(GL_BACK); GLenum native_error = glGetError();
        nb_glReadBuffer(GL_BACK);
        check(native_error != GL_NO_ERROR && glGetError() == native_error, "user FBO read validation changed");
        glDrawBuffers(1, &back); native_error = glGetError();
        nb_glDrawBuffers(1, &back);
        check(native_error != GL_NO_ERROR && glGetError() == native_error, "user FBO draw validation changed");
        glInvalidateFramebuffer(GL_FRAMEBUFFER, 3, attachments); native_error = glGetError();
        nb_glInvalidateFramebuffer(GL_FRAMEBUFFER, 3, attachments);
        check(native_error != GL_NO_ERROR && glGetError() == native_error, "user FBO invalidate validation changed");
        bionic_glBindFramebuffer(GL_DRAW_FRAMEBUFFER, names[2]);
        bionic_glBindFramebuffer(GL_READ_FRAMEBUFFER, 0);
        nb_glReadBuffer(GL_BACK);
        nb_glInvalidateFramebuffer(GL_READ_FRAMEBUFFER, 3, attachments);
        check(glGetError() == GL_NO_ERROR, "separate default read mapping failed");
        bionic_glBindFramebuffer(GL_FRAMEBUFFER, 0);
    }
    /* A real pbuffer or direct-EGL context must retain native query semantics. */
    g_hash_table_remove(draw_surface_hashtable, context);
    query(GL_FRAMEBUFFER_BINDING, (GLint)second.gl_framebuffer);
    g_hash_table_insert(draw_surface_hashtable, context, &surface);
    surface.back_buffer = NULL;
    query(GL_FRAMEBUFFER_BINDING, (GLint)second.gl_framebuffer);
    surface.back_buffer = &second;
    setenv("ATL_DIRECT_EGL", "1", 1);
    query(GL_FRAMEBUFFER_BINDING, (GLint)second.gl_framebuffer);
    unsetenv("ATL_DIRECT_EGL");
    query(GL_FRAMEBUFFER_BINDING, 0);
    glUseProgram(0); query(GL_CURRENT_PROGRAM, 0);
    nb_glGetIntegerv(GL_NUM_EXTENSIONS, &physical);
    check(physical > 0, "ARM extension count handling regressed");
    check(glGetError() == GL_NO_ERROR, "extension query GL error");
    glBindFramebuffer(GL_FRAMEBUFFER, 0); glDeleteFramebuffers(4, names); glDeleteTextures(4, textures);
    g_hash_table_unref(draw_surface_hashtable); draw_surface_hashtable = NULL;
    check(eglMakeCurrent(display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT), "release context");
    eglDestroyContext(display, context);
    printf("FRAMEBUFFER_BINDING_OK requested_es=%d actual_es3=%d jni=1 arm=1 rotation=1 read_draw=1 direct_passthrough=1\n",
           version, es3);
}
int main(int argc, char **argv) {
    check(argc == 2, "expected requested ES version");
    int version = atoi(argv[1]);
    unsetenv("ATL_DIRECT_EGL");
    PFNEGLGETPLATFORMDISPLAYEXTPROC get_display =
        (PFNEGLGETPLATFORMDISPLAYEXTPROC)eglGetProcAddress("eglGetPlatformDisplayEXT");
    check(get_display != NULL, "surfaceless EGL entry point unavailable");
    EGLDisplay display = get_display(EGL_PLATFORM_SURFACELESS_MESA, EGL_DEFAULT_DISPLAY, NULL);
    check(display != EGL_NO_DISPLAY && eglInitialize(display, NULL, NULL), "surfaceless display unavailable");
    check(eglBindAPI(EGL_OPENGL_ES_API), "bind GLES API");
    const EGLint config_attributes[] = {EGL_SURFACE_TYPE, EGL_PBUFFER_BIT,
        EGL_RENDERABLE_TYPE, EGL_OPENGL_ES2_BIT, EGL_RED_SIZE, 8, EGL_NONE};
    EGLConfig config; EGLint count;
    check(eglChooseConfig(display, config_attributes, &config, 1, &count) && count, "pbuffer config");
    const EGLint pbuffer_attributes[] = {EGL_WIDTH, 1, EGL_HEIGHT, 1, EGL_NONE};
    EGLSurface pbuffer = eglCreatePbufferSurface(display, config, pbuffer_attributes);
    check(pbuffer != EGL_NO_SURFACE, "pbuffer creation");
    run_context(display, config, pbuffer, version);
    eglDestroySurface(display, pbuffer); eglTerminate(display); return 0;
}
'''


def main():
    javac = shutil.which('javac')
    if not javac:
        raise RuntimeError('javac is needed to locate jni.h')
    java_home = pathlib.Path(javac).resolve().parents[1]
    egl = (ATL/'libandroid/egl.c').read_text()
    jni = (ATL/'api-impl-jni/android_opengl_GLES20.c').read_text()
    # Include the real ARM query and extension filter implementation, with its
    # include directives supplied by the harness and GLES_bio.h include path.
    arm = (NB/'libGLESv2.itf.c').read_text()
    functions = '\n'.join([extract(egl, 'bionic_glBindFramebuffer'),
                           extract(egl, 'bionic_glGetIntegerv'),
                           extract(egl, 'rotate_default_framebuffer'),
                           extract(jni, 'Java_android_opengl_GLES20_glGetIntegerv__I_3II'), arm])
    flags = shlex.split(subprocess.check_output(
        ['pkg-config', '--cflags', '--libs', 'egl', 'glesv2', 'glib-2.0'], text=True))
    with tempfile.TemporaryDirectory(prefix='clbb-framebuffer-binding-') as temporary:
        directory = pathlib.Path(temporary)
        source = directory/'check.c'; source.write_text(ADAPTER + functions + FIXTURE)
        binary = directory/'check'
        subprocess.run([*shlex.split(os.environ.get('CC', 'cc')), '-std=gnu11', '-O1', '-g',
                        '-Wall', '-Wextra', '-Werror', '-Wno-unused-parameter',
                        '-I' + str(java_home/'include'), '-I' + str(java_home/'include/linux'),
                        '-I' + str(NB), str(source), *flags, '-o', str(binary)], check=True)
        for version in (2, 3):
            environment = os.environ.copy()
            environment.update(EGL_PLATFORM='surfaceless', LIBGL_ALWAYS_SOFTWARE='1', GALLIUM_DRIVER='llvmpipe',
                               MESA_GLES_VERSION_OVERRIDE='2.0' if version == 2 else '3.0')
            result = subprocess.run([str(binary), str(version)], env=environment, capture_output=True,
                                    text=True, timeout=30)
            if result.returncode:
                raise RuntimeError(result.stderr + result.stdout)
            print(result.stdout.strip())


if __name__ == '__main__':
    main()
