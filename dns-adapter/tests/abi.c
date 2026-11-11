#define _GNU_SOURCE
#include "clbb_dns.h"
#include <assert.h>
#include <errno.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <linux/filter.h>
#include <linux/seccomp.h>
#include <sys/prctl.h>
#include <sys/syscall.h>

_Static_assert(sizeof(clbb_dns_hints_v1) == 20, "hints ABI");
_Static_assert(sizeof(clbb_dns_record_v1) == 40, "record ABI");
_Static_assert(offsetof(clbb_dns_record_v1, address) == 24, "address ABI");
_Static_assert(offsetof(clbb_dns_response_v1, records) == 1040, "records ABI");
_Static_assert(sizeof(clbb_dns_response_v1) == 3600, "response ABI");

/* Every test runs with all socket creation, connection and send syscalls
 * denied. NSS may fall back from local services to /etc/hosts. No public
 * hostname is requested, and a test cannot generate DNS traffic. */
#define DENY(n) BPF_JUMP(BPF_JMP | BPF_JEQ | BPF_K, (n), 0, 1), \
    BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ERRNO | EPERM)
static void deny_network(void) {
    struct sock_filter code[] = {
        BPF_STMT(BPF_LD | BPF_W | BPF_ABS, offsetof(struct seccomp_data, nr)),
        DENY(__NR_socket), DENY(__NR_socketpair), DENY(__NR_connect),
        DENY(__NR_sendto), DENY(__NR_sendmsg), DENY(__NR_sendmmsg),
        BPF_STMT(BPF_RET | BPF_K, SECCOMP_RET_ALLOW),
    };
    struct sock_fprog program = {sizeof(code) / sizeof(code[0]), code};
    assert(prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) == 0);
    assert(prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, &program) == 0);
}

int main(void) {
    deny_network();
    clbb_dns_hints_v1 hints = {
        CLBB_DNS_VERSION, CLBB_DNS_NUMERICHOST | CLBB_DNS_NUMERICSERV,
        4, 1, 6
    };
    clbb_dns_response_v1 out;
    assert(clbb_dns_resolve_v1("192.0.2.1", "443", &hints, &out) == 0);
    assert(out.version == 1 && out.count == 1 && out.canonical_len == 0);
    assert(out.records[0].family == 4 && out.records[0].port == 443);
    assert(out.records[0].socktype == 1 && out.records[0].protocol == 6);
    assert(memcmp(out.records[0].address, (unsigned char[]){192, 0, 2, 1}, 4) == 0);
    for (unsigned i = 4; i < 16; ++i) assert(out.records[0].address[i] == 0);

    hints.family = 6; hints.socktype = 2; hints.protocol = 17;
    assert(clbb_dns_resolve_v1("2001:db8::1", "53", &hints, &out) == 0);
    assert(out.count == 1 && out.records[0].port == 53);
    assert(out.records[0].family == 6 && out.records[0].protocol == 17);
    assert(memcmp(out.records[0].address,
        (unsigned char[]){0x20, 1, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1}, 16) == 0);
    assert(clbb_dns_resolve_v1("fe80::1%7", "53", &hints, &out) == 0);
    assert(out.records[0].scope_id == 7);

    hints.family = 0; hints.socktype = 1; hints.protocol = 6;
    hints.flags = CLBB_DNS_CANONNAME | CLBB_DNS_NUMERICSERV;
    assert(clbb_dns_resolve_v1("localhost", "443", &hints, &out) == 0);
    assert(out.count > 0 && out.canonical_len > 0);
    assert(out.canonical_len < CLBB_DNS_CANON_CAPACITY);
    assert(out.canonical[out.canonical_len] == 0);
    for (unsigned i = 0; i < out.count; ++i) {
        clbb_dns_record_v1 *r = &out.records[i];
        assert(r->port == 443 && r->socktype == 1 && r->protocol == 6);
        if (r->family == 4) assert(r->address[0] == 127);
        else {
            assert(r->family == 6);
            assert(memcmp(r->address,
                (unsigned char[]){0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1}, 16) == 0);
        }
    }

    hints.flags = CLBB_DNS_NUMERICHOST | CLBB_DNS_NUMERICSERV;
    assert(clbb_dns_resolve_v1("localhost", "443", &hints, &out) == CLBB_DNS_NONAME);
    assert(out.count == 0 && out.canonical_len == 0 && out.canonical[0] == 0);
    assert(clbb_dns_resolve_v1("127.0.0.1", "https", &hints, &out) == CLBB_DNS_NONAME);
    hints.flags |= 0x80000000u;
    assert(clbb_dns_resolve_v1("127.0.0.1", "443", &hints, &out) == CLBB_DNS_BADFLAGS);
    hints.flags = CLBB_DNS_PASSIVE | CLBB_DNS_NUMERICSERV;
    hints.family = 4;
    assert(clbb_dns_resolve_v1(NULL, "80", &hints, &out) == 0);
    assert(out.count == 1 && out.records[0].port == 80 && out.records[0].address[0] == 0);
    hints.family = 99;
    assert(clbb_dns_resolve_v1(NULL, "80", &hints, &out) == CLBB_DNS_FAMILY);
    hints.family = 4; hints.protocol = 17;
    assert(clbb_dns_resolve_v1(NULL, "80", &hints, &out) == CLBB_DNS_BADHINTS);
    assert(clbb_dns_resolve_v1(NULL, "80", NULL, &out) == CLBB_DNS_BADHINTS);
    assert(clbb_dns_resolve_v1(NULL, "80", &hints, NULL) == CLBB_DNS_BADHINTS);
    puts("CLBB_DNS_ADAPTER_OK=1 network_syscalls=denied");
}
