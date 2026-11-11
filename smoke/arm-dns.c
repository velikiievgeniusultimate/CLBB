#include <jni.h>
#include <arpa/inet.h>
#include <errno.h>
#include <netdb.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

enum { WORKERS = 2, ITERATIONS = 12, EXPECTED = 125 };

#define REQUIRE(test, code) do { if (!(test)) return -(code); } while (0)

static int address_list(const char* node, int family, int flags,
                        int type, int protocol, unsigned port, int* has_ipv6) {
    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = family;
    hints.ai_flags = flags | AI_NUMERICSERV;
    hints.ai_socktype = type;
    hints.ai_protocol = protocol;
    char service[8];
    snprintf(service, sizeof(service), "%u", port);
    struct addrinfo* list = NULL;
    int status = getaddrinfo(node, service, &hints, &list);
    if (status || !list) fprintf(stderr, "DNS address_list node=%s status=%d errno=%d message=%s\n", node, status, errno, gai_strerror(status));
    REQUIRE(status == 0 && list != NULL, 10);
    int error = 0;
    if ((flags & AI_CANONNAME) && (!list->ai_canonname || !list->ai_canonname[0])) error = 11;
    unsigned count = 0;
    for (struct addrinfo* ai = list; ai; ai = ai->ai_next) {
        if (++count > 64) { error = 12; break; }
        if (ai->ai_socktype != type || ai->ai_protocol != protocol || !ai->ai_addr) {
            error = 13; break;
        }
        unsigned actual_port = 0;
        const void* bytes = NULL;
        if (ai->ai_family == AF_INET && ai->ai_addrlen >= sizeof(struct sockaddr_in)) {
            struct sockaddr_in* sa = (struct sockaddr_in*)ai->ai_addr;
            if (sa->sin_family != AF_INET) { error = 14; break; }
            actual_port = ntohs(sa->sin_port);
            bytes = &sa->sin_addr;
            if (!strcmp(node, "localhost") && ((const unsigned char*)bytes)[0] != 127) {
                error = 15; break;
            }
        } else if (ai->ai_family == AF_INET6 && ai->ai_addrlen >= sizeof(struct sockaddr_in6)) {
            struct sockaddr_in6* sa = (struct sockaddr_in6*)ai->ai_addr;
            if (sa->sin6_family != AF_INET6) { error = 16; break; }
            actual_port = ntohs(sa->sin6_port);
            bytes = &sa->sin6_addr;
            if (has_ipv6) *has_ipv6 = 1;
            if (!strcmp(node, "localhost") && memcmp(bytes,
                (const unsigned char[]){0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1}, 16)) {
                error = 17; break;
            }
        } else { error = 18; break; }
        if (family != AF_UNSPEC && ai->ai_family != family) { error = 19; break; }
        if (actual_port != port) { error = 20; break; }
        if (flags & AI_NUMERICHOST) {
            unsigned char expected[16];
            if (inet_pton(family, node, expected) != 1 ||
                memcmp(bytes, expected, family == AF_INET ? 4 : 16)) { error = 21; break; }
        }
    }
    freeaddrinfo(list);
    return error ? -error : 0;
}

static int legacy(const char* node, int family, int optional) {
    /* Intentionally unaligned caller buffer to check pointer-array alignment. */
    unsigned char storage[4097];
    struct hostent host;
    struct hostent* result = NULL;
    int host_error = 0;
    int status = gethostbyname2_r(node, family, &host, (char*)storage + 1, 4096,
                                &result, &host_error);
    if (optional && !result && (host_error == HOST_NOT_FOUND ||
            host_error == NO_DATA || host_error == NO_RECOVERY)) return 0;
    REQUIRE(status == 0 && result == &host && host_error == NETDB_SUCCESS, 30);
    REQUIRE(host.h_addrtype == family && host.h_length == (family == AF_INET ? 4 : 16), 31);
    REQUIRE(host.h_name && host.h_name[0] && host.h_aliases && host.h_addr_list, 32);
    REQUIRE(((uintptr_t)host.h_aliases % sizeof(char*)) == 0, 33);
    REQUIRE(((uintptr_t)host.h_addr_list % sizeof(char*)) == 0, 34);
    REQUIRE(host.h_addr_list[0] != NULL, 35);
    unsigned char expected[16];
    int numeric = inet_pton(family, node, expected) == 1;
    unsigned count = 0;
    for (char** addr = host.h_addr_list; *addr; ++addr) {
        REQUIRE(++count <= 64, 36);
        REQUIRE((uintptr_t)*addr >= (uintptr_t)storage + 1 &&
            (uintptr_t)*addr + host.h_length <= (uintptr_t)storage + sizeof(storage), 37);
        if (numeric) REQUIRE(memcmp(*addr, expected, host.h_length) == 0, 38);
        else if (family == AF_INET) REQUIRE(((unsigned char*)*addr)[0] == 127, 39);
        else REQUIRE(memcmp(*addr,
            (const unsigned char[]){0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1}, 16) == 0, 40);
    }
    return 0;
}

static int errors(void) {
    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_INET;
    hints.ai_socktype = SOCK_STREAM;
    hints.ai_flags = AI_NUMERICHOST | AI_NUMERICSERV;
    struct addrinfo* result = NULL;
    REQUIRE(getaddrinfo("localhost", "443", &hints, &result) == EAI_NONAME, 50);
    REQUIRE(result == NULL, 51);
    hints.ai_flags = AI_NUMERICSERV;
    REQUIRE(getaddrinfo("localhost", "https", &hints, &result) != 0, 52);
    REQUIRE(result == NULL, 53);
    struct hostent host;
    struct hostent* legacy_result = NULL;
    char tiny[1];
    int host_error = 0;
    errno = 0;
    int status = gethostbyname2_r("127.0.0.1", AF_INET, &host, tiny, sizeof(tiny),
                                &legacy_result, &host_error);
    REQUIRE(status == ERANGE && errno == ERANGE && legacy_result == NULL &&
        host_error == NETDB_INTERNAL, 54);
    return 0;
}

struct worker_state { int ipv6_localhost; int result; unsigned completed; };

static int iteration(int ipv6_localhost) {
    int result = address_list("localhost", AF_UNSPEC, AI_CANONNAME,
                             SOCK_STREAM, IPPROTO_TCP, 443, NULL);
    if (result) return result;
    result = legacy("localhost", AF_INET, 0);
    if (result) return result;
    result = legacy("localhost", AF_INET6, !ipv6_localhost);
    if (result) return result;
    result = legacy("127.0.0.1", AF_INET, 0);
    if (result) return result;
    return legacy("::1", AF_INET6, 0);
}

static void* worker(void* pointer) {
    struct worker_state* state = pointer;
    for (unsigned i = 0; i < ITERATIONS; ++i) {
        state->result = iteration(state->ipv6_localhost);
        if (state->result) return NULL;
        ++state->completed;
    }
    return NULL;
}

JNIEXPORT jint JNICALL Java_CLBBArmDns_check(JNIEnv* env, jclass cls) {
    (void)env; (void)cls;
    int ipv6_localhost = 0;
    int result = address_list("localhost", AF_UNSPEC, AI_CANONNAME,
                             SOCK_STREAM, IPPROTO_TCP, 443, &ipv6_localhost);
    if (result) return result;
    result = address_list("192.0.2.1", AF_INET, AI_NUMERICHOST,
                          SOCK_STREAM, IPPROTO_TCP, 8443, NULL);
    if (result) return result;
    result = address_list("2001:db8::1", AF_INET6, AI_NUMERICHOST,
                          SOCK_DGRAM, IPPROTO_UDP, 5353, NULL);
    if (result) return result;
    result = errors();
    if (result) return result;
    result = iteration(ipv6_localhost);
    if (result) return result;

    pthread_t threads[WORKERS];
    struct worker_state states[WORKERS];
    memset(states, 0, sizeof(states));
    unsigned started = 0;
    for (; started < WORKERS; ++started) {
        states[started].ipv6_localhost = ipv6_localhost;
        if (pthread_create(&threads[started], NULL, worker, &states[started])) break;
    }
    int failed = started != WORKERS;
    for (unsigned i = 0; i < started; ++i) {
        if (pthread_join(threads[i], NULL) || states[i].result ||
            states[i].completed != ITERATIONS) failed = 1;
    }
    REQUIRE(!failed, 60);
    return EXPECTED;
}
