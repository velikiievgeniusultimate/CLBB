#!/usr/bin/env python3
"""Exercise production AudioTrack open-failure branches without an audio device."""
import argparse
import os
import pathlib
import re
import shlex
import shutil
import subprocess
import tempfile


ROOT = pathlib.Path(__file__).resolve().parents[1]
SOURCE = pathlib.Path('src/api-impl-jni/audio/android_media_AudioTrack.c')
FUNCTIONS = ('Java_android_media_AudioTrack_getMinBufferSize',
             'Java_android_media_AudioTrack_native_1constructor')


def extract_functions(source):
    # Mask comments/literals for brace matching; compile the original text intact.
    tokens = r'"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'|/\*.*?\*/|//[^\n]*'
    masked = re.sub(tokens, lambda match: ' ' * len(match.group()), source, flags=re.S)
    bodies = []
    for name in FUNCTIONS:
        match = re.search(r'^JNIEXPORT\s+\w+\s+JNICALL\s+' + re.escape(name)
                          + r'\s*\([^;{}]*\)\s*\{', masked, re.M)
        if match is None:
            raise RuntimeError('Production JNI function not found: ' + name)
        depth = 1
        end = match.end()
        while depth and end < len(masked):
            depth += (masked[end] == '{') - (masked[end] == '}')
            end += 1
        if depth:
            raise RuntimeError('Unbalanced production function: ' + name)
        bodies.append(source[match.start():end])
    return '\n\n'.join(bodies)


ADAPTER = r'''
#define _GNU_SOURCE
#include <alsa/asoundlib.h>
#include <alloca.h>
#include <jni.h>
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define PCM_DEVICE "default"
#define _INTPTR(pointer) ((intptr_t)(pointer))
static unsigned opens, helpers, writes, lookups, throws, deletions;
static jboolean pending;
static jclass exception_class = (jclass)(uintptr_t)0x1234;
static void require(int condition, const char *message) {
    if (!condition) { fprintf(stderr, "AUDIOTRACK_TEST_FAILURE: %s\n", message); exit(86); }
}
static void forbidden_write(void) {
    ++writes;
    require(0, "JNI state write after failed snd_pcm_open");
}
#define _SET_LONG_FIELD(object, name, value) forbidden_write()
#define _SET_INT_FIELD(object, name, value) forbidden_write()
#define _SET_STATIC_INT_FIELD(object, name, value) forbidden_write()

int __wrap_snd_pcm_open(snd_pcm_t **handle, const char *name,
                        snd_pcm_stream_t stream, int mode) {
    ++opens;
    require(name && !strcmp(name, PCM_DEVICE), "unexpected PCM name");
    require(stream == SND_PCM_STREAM_PLAYBACK && mode == 0, "unexpected PCM open flags");
    /* A failed ALSA call must never make this deliberately invalid pointer usable. */
    *handle = (snd_pcm_t *)(uintptr_t)0xdeadbeef;
    return -EBUSY;
}
void helper_hw_params_init(snd_pcm_t *handle, snd_pcm_hw_params_t *params,
                          unsigned int rate, int config, snd_pcm_format_t format,
                          unsigned int *channels) {
    (void)handle; (void)params; (void)rate; (void)config; (void)format;
    /* Define this adapter's output for compiler dataflow; the call still fails. */
    if (channels) *channels = 2;
    ++helpers;
    require(0, "hardware helper reached after failed snd_pcm_open");
}
static jclass find_class(JNIEnv *env, const char *name) {
    (void)env; ++lookups;
    require(!strcmp(name, "java/lang/UnsupportedOperationException"), "wrong Java exception class");
    return exception_class;
}
static jint throw_new(JNIEnv *env, jclass type, const char *message) {
    (void)env; ++throws;
    require(type == exception_class, "wrong exception class handle");
    require(message && *message, "empty audio-open failure message");
    pending = JNI_TRUE;
    return JNI_OK;
}
static void delete_local_ref(JNIEnv *env, jobject reference) {
    (void)env; ++deletions;
    require(reference == (jobject)exception_class, "unexpected local reference");
}
static jboolean exception_check(JNIEnv *env) { (void)env; return pending; }
static void exception_describe(JNIEnv *env) { (void)env; }
static const struct JNINativeInterface_ jni = {
    .FindClass = find_class, .ThrowNew = throw_new, .DeleteLocalRef = delete_local_ref,
    .ExceptionCheck = exception_check, .ExceptionDescribe = exception_describe
};
'''

FIXTURE = r'''
int main(int argc, char **argv) {
    JNIEnv env = &jni;
    int query_only = argc == 2 && !strcmp(argv[1], "query");
    int constructor_only = argc == 2 && !strcmp(argv[1], "constructor");
    require(argc == 1 || query_only || constructor_only, "unexpected test argument");
    for (unsigned iteration = 0; !constructor_only && iteration < 32; ++iteration) {
        unsigned before = opens;
        jint size = Java_android_media_AudioTrack_getMinBufferSize(
            &env, NULL, 48000, 12, 2);
        require(size == -1, "getMinBufferSize must return ERROR (-1)");
        require(opens == before + 1, "query must attempt exactly one open");
        require(!helpers && !writes && !throws && !lookups && !pending,
                "query changed JNI state or touched the failed handle");
    }
    for (unsigned iteration = 0; !query_only && iteration < 32; ++iteration) {
        unsigned before = opens;
        pending = JNI_FALSE;
        Java_android_media_AudioTrack_native_1constructor(
            &env, NULL, 3, 48000, 12, 2, 4096, 1);
        require(opens == before + 1, "constructor must attempt exactly one open");
        require(!helpers && !writes, "constructor used the failed handle or wrote state");
        require(lookups == iteration + 1 && throws == iteration + 1 && pending,
                "constructor must leave UnsupportedOperationException pending");
    }
    printf("AUDIOTRACK_OPEN_FAILURE_OK queries=%u constructors=%u opens=%u helpers=%u writes=%u exceptions=%u\n",
           constructor_only ? 0u : 32u, query_only ? 0u : 32u, opens, helpers, writes, throws);
    return 0;
}
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sanitizers', action='store_true', help='also run ASan/UBSan')
    parser.add_argument('--baseline-ref', help='also prove an unfixed ATL Git revision fails')
    args = parser.parse_args()
    javac = shutil.which('javac')
    if not javac:
        raise RuntimeError('javac is needed to locate jni.h')
    java_home = pathlib.Path(javac).resolve().parents[1]
    alsa_flags = shlex.split(subprocess.check_output(
        ['pkg-config', '--cflags', '--libs', 'alsa'], text=True))
    compiler = shlex.split(os.environ.get('CC', 'cc'))
    current = (ROOT/'src/atl'/SOURCE).read_text()
    versions = [('current', current, False)]
    if args.baseline_ref:
        baseline = subprocess.check_output(['git', 'show', args.baseline_ref + ':' + str(SOURCE)],
                                           cwd=ROOT/'src/atl', text=True)
        versions.insert(0, ('baseline', baseline, True))
    with tempfile.TemporaryDirectory(prefix='clbb-audiotrack-open-failure-') as temporary:
        directory = pathlib.Path(temporary)
        for label, contents, expected_failure in versions:
            source = directory/(label + '.c')
            source.write_text(ADAPTER + extract_functions(contents) + FIXTURE)
            modes = ['plain', 'asan-ubsan'] if args.sanitizers and not expected_failure else ['plain']
            for mode in modes:
                binary = directory/(label + '-' + mode)
                flags = ['-std=c11', '-O1', '-g', '-Wall', '-Wextra',
                         '-Wno-unused-parameter', '-Wno-unused-variable', '-Wno-unused-but-set-variable',
                         '-I' + str(java_home/'include'), '-I' + str(java_home/'include/linux')]
                if mode == 'asan-ubsan':
                    flags += ['-fsanitize=address,undefined', '-fno-omit-frame-pointer']
                subprocess.run([*compiler, *flags, str(source), '-Wl,--wrap=snd_pcm_open',
                                *alsa_flags, '-o', str(binary)], check=True)
                environment = os.environ.copy()
                environment.update(ASAN_OPTIONS='detect_leaks=1:halt_on_error=1',
                                   UBSAN_OPTIONS='halt_on_error=1:print_stacktrace=1')
                if expected_failure:
                    for case in ('query', 'constructor'):
                        result = subprocess.run([str(binary), case], env=environment, text=True,
                                                capture_output=True, timeout=10)
                        if result.returncode != 86 or 'hardware helper reached' not in result.stderr:
                            raise AssertionError('Baseline did not reproduce ' + case + ' misuse: '
                                                 + result.stderr)
                        print('AUDIOTRACK_BASELINE_REPRODUCED=' + case)
                else:
                    result = subprocess.run([str(binary)], env=environment, text=True,
                                            capture_output=True, timeout=10)
                    if result.returncode:
                        raise AssertionError(mode + ': ' + result.stderr + result.stdout)
                    marker = next(line for line in result.stdout.splitlines()
                                  if line.startswith('AUDIOTRACK_OPEN_FAILURE_OK'))
                    print(mode + ': ' + marker)


if __name__ == '__main__':
    main()
