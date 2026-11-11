/* Self-authored, game-independent ARM64 Bionic semantic regression fixture. */
#include <jni.h>
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <errno.h>
#include <limits.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>
#include <sched.h>

/* Indirection and -fno-builtin keep these calls on the actual guest libc path. */
static void *(*volatile copy_call)(void *, const void *, size_t) = memcpy;
static void *(*volatile move_call)(void *, const void *, size_t) = memmove;
static void *(*volatile set_call)(void *, int, size_t) = memset;
static size_t (*volatile length_call)(const char *) = strlen;
static int (*volatile compare_call)(const char *, const char *) = strcmp;
static void *(*volatile alloc_call)(size_t) = malloc;
static void (*volatile free_call)(void *) = free;

#define CHECK(c) do { if (!(c)) { printf("CLBB_LIBC_FAILURE_LINE=%d\n", __LINE__); return __LINE__; } } while (0)
static unsigned char pattern(size_t i) { return (unsigned char)(i * 131u + 17u); }
static void fill(unsigned char *p, size_t n) { for (size_t i = 0; i < n; ++i) p[i] = pattern(i); }
static int has_pattern(const unsigned char *p, size_t n) {
    for (size_t i = 0; i < n; ++i) if (p[i] != pattern(i)) return 0;
    return 1;
}

static int allocator_contract(void) {
    static const size_t sizes[] = {1, 7, 16, 31, 64, 257, 4096, 65537};
    free_call(NULL);
    free_call(alloc_call(0)); /* Either NULL or a freeable pointer is valid. */
    free_call(calloc(0, 16));
    for (size_t k = 0; k < sizeof(sizes) / sizeof(sizes[0]); ++k) {
        size_t n = sizes[k];
        unsigned char *p = alloc_call(n);
        CHECK(p != NULL); fill(p, n);
        unsigned char *q = realloc(p, n * 2 + 9);
        CHECK(q != NULL && has_pattern(q, n));
        fill(q, n * 2 + 9);
        p = realloc(q, n);
        CHECK(p != NULL && has_pattern(p, n)); free_call(p);
        p = calloc(n, 3);
        CHECK(p != NULL);
        for (size_t i = 0; i < n * 3; ++i) CHECK(p[i] == 0);
        free_call(p);
    }
    unsigned char *p = realloc(NULL, 113);
    CHECK(p != NULL); fill(p, 113); free_call(p);
    volatile size_t huge = SIZE_MAX;
    errno = 0; p = calloc(huge, 2);
    CHECK(p == NULL && errno == ENOMEM);
    void *aligned = (void *)(uintptr_t)0x1234;
    CHECK(posix_memalign(&aligned, 3, 99) == EINVAL);
    CHECK(aligned == (void *)(uintptr_t)0x1234);
    for (size_t alignment = sizeof(void *); alignment <= 4096; alignment *= 2) {
        CHECK(posix_memalign(&aligned, alignment, 8193) == 0);
        CHECK(aligned != NULL && (uintptr_t)aligned % alignment == 0);
        fill(aligned, 8193); CHECK(has_pattern(aligned, 8193)); free_call(aligned);
    }
    return 0;
}

struct guarded { unsigned char *mapping; unsigned char *data; size_t page; };
static int make_guarded(struct guarded *g) {
    long page = sysconf(_SC_PAGESIZE);
    if (page < 4096) return 0;
    g->page = (size_t)page;
    g->mapping = mmap(NULL, g->page * 3, PROT_NONE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (g->mapping == MAP_FAILED) return 0;
    g->data = g->mapping + g->page;
    return mprotect(g->data, g->page, PROT_READ | PROT_WRITE) == 0;
}

static int memory_contract(void) {
    struct guarded a, b;
    CHECK(make_guarded(&a)); CHECK(make_guarded(&b));
    static const size_t lengths[] = {0, 1, 2, 7, 15, 16, 31, 32, 63, 64, 95, 96, 127, 128, 255, 256, 1023, 4096};
    for (size_t k = 0; k < sizeof(lengths) / sizeof(lengths[0]); ++k) {
        size_t n = lengths[k];
        unsigned char *src = a.data + a.page - n;
        unsigned char *dst = b.data + b.page - n;
        fill(src, n);
        CHECK(set_call(dst, 0xa7, n) == dst);
        for (size_t i = 0; i < n; ++i) CHECK(dst[i] == 0xa7);
        CHECK(copy_call(dst, src, n) == dst && has_pattern(dst, n));
        CHECK(move_call(dst, src, n) == dst && has_pattern(dst, n));
        /* Also exercise the first valid byte: no read/write before the page. */
        fill(a.data, n);
        CHECK(copy_call(b.data, a.data, n) == b.data && has_pattern(b.data, n));
    }
    /* Bionic ARM64 memcpy intentionally aliases memmove. Preserve this existing
     * libc contract without asking the C compiler to reason about overlap. */
    unsigned char reference[1024];
    for (size_t n = 1; n <= 511; n = n * 2 + 1) {
        for (size_t shift = 1; shift <= 17; shift += 4) {
            for (int reverse = 0; reverse < 2; ++reverse) {
                size_t total = n + shift;
                unsigned char *base = a.data + a.page - total;
                fill(base, total); fill(reference, total);
                if (reverse) {
                    for (size_t i = 0; i < n; ++i) reference[i] = pattern(i + shift);
                    CHECK(copy_call(base, base + shift, n) == base);
                } else {
                    for (size_t i = n; i > 0; --i) reference[shift + i - 1] = pattern(i - 1);
                    CHECK(copy_call(base + shift, base, n) == base + shift);
                }
                for (size_t i = 0; i < total; ++i) CHECK(base[i] == reference[i]);
                fill(base, total);
                if (reverse) CHECK(move_call(base, base + shift, n) == base);
                else CHECK(move_call(base + shift, base, n) == base + shift);
                for (size_t i = 0; i < total; ++i) CHECK(base[i] == reference[i]);
            }
        }
    }
    for (size_t n = 0; n <= 255; ++n) {
        unsigned char *x = a.data + a.page - n - 1;
        unsigned char *y = b.data + b.page - n - 1;
        for (size_t i = 0; i < n; ++i) x[i] = y[i] = (unsigned char)(1 + i % 255);
        x[n] = y[n] = 0;
        CHECK(length_call((char *)x) == n && length_call((char *)y) == n);
        CHECK(compare_call((char *)x, (char *)y) == 0);
        if (n) {
            x[n - 1] = 0xff; y[n - 1] = 0x80;
            CHECK(compare_call((char *)x, (char *)y) > 0);
            CHECK(compare_call((char *)y, (char *)x) < 0);
        }
    }
    CHECK(munmap(a.mapping, a.page * 3) == 0);
    CHECK(munmap(b.mapping, b.page * 3) == 0);
    return 0;
}

struct wrong_owner { pthread_mutex_t *mutex; int result; };
static void *unlock_wrong_owner(void *opaque) {
    struct wrong_owner *w = opaque; w->result = pthread_mutex_unlock(w->mutex); return NULL;
}
static int mutex_contract(void) {
    pthread_mutex_t m = PTHREAD_MUTEX_INITIALIZER;
    CHECK(pthread_mutex_lock(&m) == 0);
    CHECK(pthread_mutex_trylock(&m) == EBUSY);
    CHECK(pthread_mutex_unlock(&m) == 0);
    CHECK(pthread_mutex_trylock(&m) == 0);
    CHECK(pthread_mutex_unlock(&m) == 0);
    CHECK(pthread_mutex_destroy(&m) == 0);
    pthread_mutexattr_t attr;
    CHECK(pthread_mutexattr_init(&attr) == 0);
    CHECK(pthread_mutexattr_settype(&attr, PTHREAD_MUTEX_ERRORCHECK) == 0);
    CHECK(pthread_mutex_init(&m, &attr) == 0);
    CHECK(pthread_mutex_lock(&m) == 0);
    CHECK(pthread_mutex_lock(&m) == EDEADLK);
    struct wrong_owner w = { &m, 0 }; pthread_t thread;
    CHECK(pthread_create(&thread, NULL, unlock_wrong_owner, &w) == 0);
    CHECK(pthread_join(thread, NULL) == 0 && w.result == EPERM);
    CHECK(pthread_mutex_unlock(&m) == 0);
    CHECK(pthread_mutex_unlock(&m) == EPERM);
    CHECK(pthread_mutex_destroy(&m) == 0);
    CHECK(pthread_mutexattr_settype(&attr, PTHREAD_MUTEX_RECURSIVE) == 0);
    CHECK(pthread_mutex_init(&m, &attr) == 0);
    CHECK(pthread_mutex_lock(&m) == 0);
    CHECK(pthread_mutex_lock(&m) == 0);
    CHECK(pthread_mutex_trylock(&m) == 0);
    CHECK(pthread_mutex_unlock(&m) == 0);
    CHECK(pthread_mutex_unlock(&m) == 0);
    CHECK(pthread_mutex_unlock(&m) == 0);
    CHECK(pthread_mutex_destroy(&m) == 0);
    CHECK(pthread_mutexattr_destroy(&attr) == 0);
    return 0;
}

enum { WORKERS = 4, COUNTS = 10000 };
struct shared {
    pthread_mutex_t mutex;
    pthread_cond_t condition;
    pthread_key_t key;
    int ready, start, counter;
};
struct worker { struct shared *s; int index, failed; uintptr_t errno_address; };
static void *contend(void *opaque) {
    struct worker *w = opaque; struct shared *s = w->s;
    int token = 100 + w->index;
    if (pthread_setspecific(s->key, &token)) { w->failed = 1; return NULL; }
    w->errno_address = (uintptr_t)&errno;
    errno = EDOM + w->index;
    if (pthread_mutex_lock(&s->mutex)) { w->failed = 2; return NULL; }
    ++s->ready; pthread_cond_broadcast(&s->condition);
    while (!s->start) if (pthread_cond_wait(&s->condition, &s->mutex)) w->failed = 3;
    pthread_mutex_unlock(&s->mutex);
    for (int i = 0; i < COUNTS; ++i) {
        if (pthread_mutex_lock(&s->mutex)) w->failed = 4;
        ++s->counter;
        if (pthread_mutex_unlock(&s->mutex)) w->failed = 5;
        if ((i & 255) == 0) sched_yield();
        if (errno != EDOM + w->index || pthread_getspecific(s->key) != &token || token != 100 + w->index) w->failed = 6;
    }
    return NULL;
}
static int concurrency_contract(void) {
    struct shared s = { .mutex = PTHREAD_MUTEX_INITIALIZER, .condition = PTHREAD_COND_INITIALIZER };
    CHECK(pthread_key_create(&s.key, NULL) == 0);
    int parent_token = 917;
    CHECK(pthread_setspecific(s.key, &parent_token) == 0);
    struct worker workers[WORKERS]; pthread_t threads[WORKERS];
    uintptr_t parent_errno = (uintptr_t)&errno;
    errno = E2BIG;
    for (int i = 0; i < WORKERS; ++i) {
        workers[i] = (struct worker){ .s = &s, .index = i };
        CHECK(pthread_create(&threads[i], NULL, contend, &workers[i]) == 0);
    }
    CHECK(pthread_mutex_lock(&s.mutex) == 0);
    while (s.ready < WORKERS) CHECK(pthread_cond_wait(&s.condition, &s.mutex) == 0);
    s.start = 1;
    CHECK(pthread_cond_broadcast(&s.condition) == 0);
    CHECK(pthread_mutex_unlock(&s.mutex) == 0);
    for (int i = 0; i < WORKERS; ++i) {
        CHECK(pthread_join(threads[i], NULL) == 0 && !workers[i].failed);
        CHECK(workers[i].errno_address != parent_errno);
        for (int j = 0; j < i; ++j) CHECK(workers[i].errno_address != workers[j].errno_address);
    }
    CHECK(s.counter == WORKERS * COUNTS && errno == E2BIG);
    CHECK(pthread_getspecific(s.key) == &parent_token);
    CHECK(pthread_key_delete(s.key) == 0);
    CHECK(pthread_mutex_lock(&s.mutex) == 0);
    struct timespec deadline;
    CHECK(clock_gettime(CLOCK_REALTIME, &deadline) == 0);
    deadline.tv_nsec += 20000000;
    if (deadline.tv_nsec >= 1000000000) { ++deadline.tv_sec; deadline.tv_nsec -= 1000000000; }
    int result;
    do { result = pthread_cond_timedwait(&s.condition, &s.mutex, &deadline); } while (result == 0);
    CHECK(result == ETIMEDOUT);
    CHECK(pthread_mutex_trylock(&s.mutex) == EBUSY); /* Timed wait reacquired it. */
    CHECK(pthread_mutex_unlock(&s.mutex) == 0);
    CHECK(pthread_cond_destroy(&s.condition) == 0);
    CHECK(pthread_mutex_destroy(&s.mutex) == 0);
    return 0;
}

static uint64_t now_ns(void) {
    struct timespec t; if (clock_gettime(CLOCK_MONOTONIC, &t)) return 0;
    return (uint64_t)t.tv_sec * 1000000000u + (uint64_t)t.tv_nsec;
}
static int microbench(void) {
    pthread_mutex_t m = PTHREAD_MUTEX_INITIALIZER;
    enum { MUTEX_PAIRS = 30000, ALLOC_PAIRS = 6000 };
    uint64_t begin = now_ns(); CHECK(begin);
    for (int i = 0; i < MUTEX_PAIRS; ++i) {
        CHECK(pthread_mutex_lock(&m) == 0); CHECK(pthread_mutex_unlock(&m) == 0);
    }
    uint64_t end = now_ns(); CHECK(end >= begin);
    printf("CLBB_LIBC_MUTEX_PAIRS=%d\nCLBB_LIBC_MUTEX_NS_PER_PAIR=%llu\n", MUTEX_PAIRS,
           (unsigned long long)((end - begin) / MUTEX_PAIRS));
    CHECK(pthread_mutex_destroy(&m) == 0);
    begin = now_ns(); CHECK(begin);
    uintptr_t observed = 0;
    for (int i = 0; i < ALLOC_PAIRS; ++i) {
        unsigned char *p = alloc_call(64 + (i & 1) * 192); CHECK(p);
        p[0] = (unsigned char)i; observed ^= (uintptr_t)p ^ p[0]; free_call(p);
    }
    end = now_ns(); CHECK(end >= begin);
    printf("CLBB_LIBC_ALLOC_PAIRS=%d\nCLBB_LIBC_ALLOC_NS_PER_PAIR=%llu\n", ALLOC_PAIRS,
           (unsigned long long)((end - begin) / ALLOC_PAIRS));
    /* Observation is deliberately not a stable or externally useful address. */
    if (observed == UINTPTR_MAX) puts("CLBB_LIBC_OBSERVATION_SENTINEL=1");
    return 0;
}

JNIEXPORT jint JNICALL Java_CLBBArmLibcContract_check(JNIEnv *env, jclass klass) {
    (void)env; (void)klass;
    int result;
    if ((result = allocator_contract())) return result;
    if ((result = memory_contract())) return result;
    if ((result = mutex_contract())) return result;
    if ((result = concurrency_contract())) return result;
    if ((result = microbench())) return result;
    printf("CLBB_LIBC_CONTRACT_STAGES=5\nCLBB_LIBC_CONTENDED_COUNT=%d\n", WORKERS * COUNTS);
    fflush(stdout);
    return 0;
}
