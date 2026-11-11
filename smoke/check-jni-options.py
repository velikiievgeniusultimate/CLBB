#!/usr/bin/env python3
"""Exercise the production JVM option builder without loading ART or the game."""
from pathlib import Path
import os
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
production = (root / 'src/atl/src/main-executable/main.c').read_text()
def extract(signature):
    start = production.index(signature)
    opening = production.index('{', start)
    end, depth = opening + 1, 1
    while depth:
        depth += (production[end] == '{') - (production[end] == '}')
        end += 1
    return production[start:end]

fixture = r'''
#include <glib.h>
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
typedef void JavaVM;
typedef void JNIEnv;
typedef struct { char *optionString; void *extraInfo; } JavaVMOption;
typedef struct { int version, nOptions; JavaVMOption *options; int ignoreUnrecognized; } JavaVMInitArgs;
#define JNI_VERSION_1_6 0x00010006
#define JNI_FALSE 0
#define JDWP_ARG "-XjdwpOptions:transport=dt_socket,server=y,suspend=y,address="
#define SDK_INT_ARG "-DBuild.VERSION.SDK_INT="
static int expect_check, expect_jdwp, expect_sdk, expect_extras, calls;
int JNI_CreateJavaVM(JavaVM **vm, void **env, void *argument) {
    (void)vm;
    const JavaVMInitArgs *args = argument;
    assert(args->version == JNI_VERSION_1_6);
    assert(args->nOptions == 4 + expect_check + 2 * expect_jdwp + expect_sdk + expect_extras);
    int found_check = 0, found_jdwp = 0, found_provider = 0, found_sdk = 0, found_extra = 0;
    for (int i = 0; i < args->nOptions; i++) {
        const char *s = args->options[i].optionString;
        assert(s != NULL);
        found_check += !strcmp(s, "-Xcheck:jni");
        found_jdwp += !strcmp(s, JDWP_ARG "28000");
        found_provider += !strcmp(s, "-XjdwpProvider:internal");
        found_sdk += !strcmp(s, SDK_INT_ARG "35");
        found_extra += !strcmp(s, "-Xmx192m") || !strcmp(s, "-Dclbb.test=true");
    }
    assert(found_check == expect_check && found_jdwp == expect_jdwp && found_provider == expect_jdwp);
    assert(found_sdk == expect_sdk && found_extra == expect_extras);
    *env = (void *)0x42;
    calls++;
    return 0;
}
'''
fixture += extract('char *construct_classpath(') + '\n' + extract('JNIEnv *create_vm(')
fixture += r'''
int main(void) {
    const char *states[] = {NULL, "", "0", "false", "FALSE", "1", "true"};
    for (unsigned i = 0; i < G_N_ELEMENTS(states); i++) {
        if (states[i]) setenv("ATL_CHECK_JNI", states[i], 1); else unsetenv("ATL_CHECK_JNI");
        expect_check = i >= 5;
        for (int diagnostic = 0; diagnostic < 2; diagnostic++) {
            if (diagnostic) setenv("JDWP_LISTEN", "28000", 1); else unsetenv("JDWP_LISTEN");
            expect_jdwp = expect_sdk = diagnostic;
            expect_extras = diagnostic ? 2 : 0;
            char *extras[] = {"-Xmx192m", "-Dclbb.test=true", NULL};
            JNIEnv *env = create_vm("api.jar", "game.apk:split.apk", "framework.apk", NULL,
                                   "natives", "libs", diagnostic ? "35" : NULL, diagnostic ? extras : NULL);
            assert(env == (void *)0x42);
        }
    }
    assert(calls == 14);
    puts("JNI_OPTIONS_OK cases=14 default_check_jni=off explicit_diagnostics=on jdwp_preserved=1");
    return 0;
}
'''
with tempfile.TemporaryDirectory(prefix='clbb-jni-options-') as temporary:
    directory = Path(temporary)
    source, binary = directory / 'options.c', directory / 'options'
    source.write_text(fixture)
    flags = subprocess.check_output(['pkg-config', '--cflags', '--libs', 'glib-2.0'], text=True).split()
    subprocess.run(['cc', '-O2', '-g', '-fno-omit-frame-pointer', '-fsanitize=address,undefined',
                    str(source), '-o', str(binary), *flags], check=True)
    subprocess.run([str(binary)], env={**os.environ, 'ASAN_OPTIONS':'detect_leaks=0'}, check=True, timeout=10)
