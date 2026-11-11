#!/usr/bin/env python3
"""Run production video GLES JNI functions against a real surfaceless ES2 context."""
import argparse
import os
import pathlib
import re
import shlex
import shutil
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
JNI_SOURCE = ROOT/'src/atl/src/api-impl-jni/android_opengl_GLES20.c'
FUNCTIONS = ('Java_android_opengl_GLES20_glDeleteBuffers__I_3II',
             'Java_android_opengl_GLES20_glUniform1f',
             'Java_android_opengl_GLES20_glGetProgramInfoLog',
             'Java_android_opengl_GLES20_glIsEnabled',
             'Java_android_opengl_GLES20_glFrontFace')


def extract_functions(source):
    tokens = r'"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'|/\*.*?\*/|//[^\n]*'
    masked = re.sub(tokens, lambda match: ' ' * len(match.group()), source, flags=re.S)
    result = []
    for name in FUNCTIONS:
        match = re.search(r'^JNIEXPORT\s+\w+\s+JNICALL\s+' + re.escape(name)
                          + r'\s*\([^;{}]*\)\s*\{', masked, re.M)
        if not match:
            raise RuntimeError('Production JNI function not found: ' + name)
        depth, end = 1, match.end()
        while depth and end < len(masked):
            depth += (masked[end] == '{') - (masked[end] == '}')
            end += 1
        if depth:
            raise RuntimeError('Unbalanced production function: ' + name)
        result.append(source[match.start():end])
    return '\n\n'.join(result)


ADAPTER = r'''
#define _GNU_SOURCE
#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <GLES2/gl2.h>
#include <jni.h>
#include <limits.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "android_opengl_GLES20.h"

typedef struct { jsize length; jint values[5]; } IntArray;
static unsigned array_gets, array_releases, exceptions;
static int pending;
static jclass argument_exception = (jclass)1;
static void check(int value, const char *message) {
    if (!value) { fprintf(stderr, "VIDEO_GL_TEST_FAILURE: %s\n", message); exit(1); }
}
static jsize array_length(JNIEnv *env, jarray reference) {
    (void)env; check(reference != NULL, "GetArrayLength with null array");
    return ((IntArray *)reference)->length;
}
static jint *array_elements(JNIEnv *env, jintArray reference, jboolean *is_copy) {
    (void)env; ++array_gets; check(reference != NULL, "GetIntArrayElements with null array");
    if (is_copy) *is_copy = JNI_FALSE;
    return ((IntArray *)reference)->values;
}
static void release_elements(JNIEnv *env, jintArray reference, jint *values, jint mode) {
    (void)env; ++array_releases;
    check(reference && values == ((IntArray *)reference)->values, "wrong array release");
    check(mode == JNI_ABORT, "read-only deletion input must use JNI_ABORT");
}
static jclass find_class(JNIEnv *env, const char *name) {
    (void)env;
    check(!strcmp(name, "java/lang/IllegalArgumentException"), "unexpected Java exception");
    return argument_exception;
}
static jint throw_new(JNIEnv *env, jclass type, const char *message) {
    (void)env; check(type == argument_exception && message && *message, "invalid ThrowNew");
    ++exceptions; pending = 1; return JNI_OK;
}
static jstring new_string(JNIEnv *env, const char *text) {
    (void)env; check(text != NULL, "NewStringUTF with null string");
    char *copy = strdup(text); check(copy != NULL, "test string allocation");
    return (jstring)copy;
}
static const struct JNINativeInterface_ functions = {
    .GetArrayLength = array_length, .GetIntArrayElements = array_elements,
    .ReleaseIntArrayElements = release_elements, .FindClass = find_class,
    .ThrowNew = throw_new, .NewStringUTF = new_string
};
'''

FIXTURE = r'''
static GLuint shader(GLenum type, const char *source) {
    GLuint object = glCreateShader(type);
    glShaderSource(object, 1, &source, NULL); glCompileShader(object);
    GLint compiled = 0; glGetShaderiv(object, GL_COMPILE_STATUS, &compiled);
    check(compiled, "fixture shader failed to compile"); return object;
}
static GLuint program(GLuint vertex, GLuint fragment) {
    GLuint object = glCreateProgram(); glAttachShader(object, vertex);
    glAttachShader(object, fragment); glLinkProgram(object); return object;
}
static void assert_live_sentinels(const GLuint *names) {
    check(glIsBuffer(names[0]) && glIsBuffer(names[3]) && glIsBuffer(names[4]),
          "deletion removed a buffer outside the requested array slice");
}
int main(void) {
    PFNEGLGETPLATFORMDISPLAYEXTPROC get_display =
        (PFNEGLGETPLATFORMDISPLAYEXTPROC)eglGetProcAddress("eglGetPlatformDisplayEXT");
    if (!get_display) { puts("VIDEO_GL_SKIP no surfaceless EGL entry point"); return 77; }
    EGLDisplay display = get_display(EGL_PLATFORM_SURFACELESS_MESA, EGL_DEFAULT_DISPLAY, NULL);
    if (display == EGL_NO_DISPLAY || !eglInitialize(display, NULL, NULL)) {
        puts("VIDEO_GL_SKIP no surfaceless EGL display"); return 77;
    }
    check(eglBindAPI(EGL_OPENGL_ES_API), "eglBindAPI");
    const EGLint config_attributes[] = {EGL_SURFACE_TYPE, EGL_PBUFFER_BIT,
        EGL_RENDERABLE_TYPE, EGL_OPENGL_ES2_BIT, EGL_RED_SIZE, 8, EGL_GREEN_SIZE, 8,
        EGL_BLUE_SIZE, 8, EGL_NONE};
    EGLConfig config; EGLint count = 0;
    check(eglChooseConfig(display, config_attributes, &config, 1, &count) && count,
          "ES2 pbuffer config unavailable");
    const EGLint surface_attributes[] = {EGL_WIDTH, 1, EGL_HEIGHT, 1, EGL_NONE};
    const EGLint context_attributes[] = {EGL_CONTEXT_CLIENT_VERSION, 2, EGL_NONE};
    EGLSurface surface = eglCreatePbufferSurface(display, config, surface_attributes);
    EGLContext context = eglCreateContext(display, config, EGL_NO_CONTEXT, context_attributes);
    check(surface != EGL_NO_SURFACE && context != EGL_NO_CONTEXT &&
          eglMakeCurrent(display, surface, surface, context), "create/make-current ES2 pbuffer");
    JNIEnv env = &functions;
    glDisable(GL_BLEND);
    check(!Java_android_opengl_GLES20_glIsEnabled(&env,NULL,GL_BLEND), "disabled capability query");
    glEnable(GL_BLEND);
    check(Java_android_opengl_GLES20_glIsEnabled(&env,NULL,GL_BLEND), "enabled capability query");
    Java_android_opengl_GLES20_glFrontFace(&env,NULL,GL_CW);
    GLint winding; glGetIntegerv(GL_FRONT_FACE,&winding); check(winding==GL_CW,"front-face setter");
    Java_android_opengl_GLES20_glFrontFace(&env,NULL,GL_CCW);
    glGetIntegerv(GL_FRONT_FACE,&winding); check(winding==GL_CCW,"front-face restore");
    check(glGetError()==GL_NO_ERROR,"capability/winding query introduced GL error");
    GLuint names[5]; glGenBuffers(5, names);
    for (unsigned i = 0; i < 5; ++i) {
        glBindBuffer(GL_ARRAY_BUFFER, names[i]);
        glBufferData(GL_ARRAY_BUFFER, 16, NULL, GL_STATIC_DRAW);
    }
    glBindBuffer(GL_ARRAY_BUFFER, 0);
    IntArray array = {.length = 5};
    for (unsigned i = 0; i < 5; ++i) array.values[i] = (jint)names[i];
    Java_android_opengl_GLES20_glDeleteBuffers__I_3II(&env, NULL, 2, (jintArray)&array, 1);
    check(!pending && array_gets == 1 && array_releases == 1, "valid slice JNI lifecycle");
    check(!glIsBuffer(names[1]) && !glIsBuffer(names[2]), "requested buffers were not deleted");
    assert_live_sentinels(names);
    for (unsigned i = 0; i < 5; ++i) check(array.values[i] == (jint)names[i], "input array changed");
    const jint bad_ranges[][2] = {{-1,0}, {1,-1}, {6,0}, {1,5}, {0,6}, {INT_MAX,1}, {1,INT_MAX}};
    for (unsigned i = 0; i < sizeof(bad_ranges)/sizeof(bad_ranges[0]); ++i) {
        unsigned previous_exceptions = exceptions; pending = 0;
        Java_android_opengl_GLES20_glDeleteBuffers__I_3II(
            &env, NULL, bad_ranges[i][0], (jintArray)&array, bad_ranges[i][1]);
        check(pending && exceptions == previous_exceptions + 1, "invalid range was not rejected");
        check(array_gets == 1 && array_releases == 1, "invalid range accessed array elements");
        check(glGetError() == GL_NO_ERROR, "invalid range reached GL with invalid arguments");
        assert_live_sentinels(names);
    }
    pending = 0;
    Java_android_opengl_GLES20_glDeleteBuffers__I_3II(&env, NULL, 1, NULL, 0);
    check(pending && array_gets == 1, "null array was not rejected before access");
    pending = 0;
    Java_android_opengl_GLES20_glDeleteBuffers__I_3II(&env, NULL, 0, (jintArray)&array, 5);
    check(!pending && array_gets == 1, "empty end slice must be harmless");
    assert_live_sentinels(names);

    GLuint vertex = shader(GL_VERTEX_SHADER,
        "attribute vec2 position; uniform float scale; void main(){gl_Position=vec4(position*scale,0.0,1.0);}");
    GLuint fragment = shader(GL_FRAGMENT_SHADER,
        "precision mediump float; void main(){gl_FragColor=vec4(1.0);}");
    GLuint linked = program(vertex, fragment); GLint status;
    glGetProgramiv(linked, GL_LINK_STATUS, &status); check(status, "fixture program link");
    glUseProgram(linked); GLint location = glGetUniformLocation(linked, "scale");
    check(location >= 0, "active fixture uniform");
    Java_android_opengl_GLES20_glUniform1f(&env, NULL, location, 0.375f);
    GLfloat value = 0; glGetUniformfv(linked, location, &value);
    check(fabsf(value - 0.375f) < 0.000001f, "real uniform readback differs");
    jstring good_log = Java_android_opengl_GLES20_glGetProgramInfoLog(&env, NULL, linked);
    check(good_log != NULL && !pending, "successful program log conversion"); free(good_log);

    GLuint broken_vertex = shader(GL_VERTEX_SHADER,
        "varying mediump vec4 mismatch; void main(){mismatch=vec4(1.0);gl_Position=vec4(0.0);}");
    GLuint broken_fragment = shader(GL_FRAGMENT_SHADER,
        "precision mediump float; varying mediump vec3 mismatch; void main(){gl_FragColor=vec4(mismatch,1.0);}");
    GLuint broken = program(broken_vertex, broken_fragment);
    glGetProgramiv(broken, GL_LINK_STATUS, &status); check(!status, "fixture must fail link");
    GLint log_length; glGetProgramiv(broken, GL_INFO_LOG_LENGTH, &log_length);
    check(log_length > 1, "driver supplied no link failure log");
    char *expected = calloc((size_t)log_length, 1); check(expected != NULL, "test log allocation");
    glGetProgramInfoLog(broken, log_length, NULL, expected);
    jstring failed_log = Java_android_opengl_GLES20_glGetProgramInfoLog(&env, NULL, broken);
    check(failed_log && *(char *)failed_log && !strcmp((char *)failed_log, expected),
          "production log differs from the real failed-link log");
    free(failed_log); free(expected);
    check(glGetError() == GL_NO_ERROR, "unexpected GL error");
    glUseProgram(0); glDeleteProgram(linked); glDeleteProgram(broken);
    glDeleteShader(vertex); glDeleteShader(fragment); glDeleteShader(broken_vertex); glDeleteShader(broken_fragment);
    glDeleteBuffers(5, names);
    check(eglMakeCurrent(display, EGL_NO_SURFACE, EGL_NO_SURFACE, EGL_NO_CONTEXT), "release current context");
    eglDestroyContext(display, context); eglDestroySurface(display, surface); eglTerminate(display);
    puts("VIDEO_GL_CLEANUP_OK offset_delete=1 invalid_ranges=8 uniform_readback=1 real_link_error_log=1");
    return 0;
}
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--allow-skip', action='store_true', help='allow unavailable surfaceless EGL')
    args = parser.parse_args()
    javac = shutil.which('javac')
    if not javac:
        raise RuntimeError('javac is needed to locate jni.h')
    java_home = pathlib.Path(javac).resolve().parents[1]
    flags = shlex.split(subprocess.check_output(['pkg-config', '--cflags', '--libs', 'egl', 'glesv2'], text=True))
    with tempfile.TemporaryDirectory(prefix='clbb-video-gl-cleanup-') as temporary:
        directory = pathlib.Path(temporary)
        source = directory/'check.c'
        source.write_text(ADAPTER + extract_functions(JNI_SOURCE.read_text()) + FIXTURE)
        binary = directory/'check'
        subprocess.run([*shlex.split(os.environ.get('CC', 'cc')), '-std=gnu11', '-O1', '-g',
                        '-Wall', '-Wextra', '-Werror', '-Wno-unused-parameter',
                        '-I' + str(java_home/'include'), '-I' + str(java_home/'include/linux'),
                        '-I' + str(JNI_SOURCE.parent/'generated_headers'), str(source),
                        *flags, '-lm', '-o', str(binary)], check=True)
        environment = os.environ.copy()
        environment.update(EGL_PLATFORM='surfaceless', LIBGL_ALWAYS_SOFTWARE='1', GALLIUM_DRIVER='llvmpipe')
        result = subprocess.run([str(binary)], env=environment, text=True, capture_output=True, timeout=30)
        if result.returncode == 77 and args.allow_skip:
            print(result.stdout.strip())
        elif result.returncode:
            raise RuntimeError(result.stderr + result.stdout)
        else:
            print(result.stdout.strip())


if __name__ == '__main__':
    main()
