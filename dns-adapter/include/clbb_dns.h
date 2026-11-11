#ifndef CLBB_DNS_H
#define CLBB_DNS_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif

/* v1 payload integers are little endian. No native pointers occur in payloads.
 * Public C function arguments are HOST pointers, not unchecked guest pointers. */
#define CLBB_DNS_VERSION 1u
#define CLBB_DNS_MAX_RECORDS 64u
#define CLBB_DNS_CANON_CAPACITY 1024u
enum {
    CLBB_DNS_PASSIVE = 1, CLBB_DNS_CANONNAME = 2,
    CLBB_DNS_NUMERICHOST = 4, CLBB_DNS_NUMERICSERV = 8,
    CLBB_DNS_ADDRCONFIG = 16, CLBB_DNS_V4MAPPED = 32, CLBB_DNS_ALL = 64
};
/* These are guest bionic EAI values; do not use host EAI constants. */
enum {
    CLBB_DNS_OK = 0, CLBB_DNS_ADDRFAMILY = 1, CLBB_DNS_AGAIN = 2, CLBB_DNS_BADFLAGS = 3,
    CLBB_DNS_FAIL = 4, CLBB_DNS_FAMILY = 5, CLBB_DNS_MEMORY = 6,
    CLBB_DNS_NODATA = 7, CLBB_DNS_NONAME = 8, CLBB_DNS_SERVICE = 9,
    CLBB_DNS_SOCKTYPE = 10, CLBB_DNS_SYSTEM = 11, CLBB_DNS_BADHINTS = 12,
    CLBB_DNS_PROTOCOL = 13, CLBB_DNS_OVERFLOW = 14
};
typedef struct {
    uint32_t version, flags;
    uint32_t family;   /* 0 unspecified, 4 IPv4, 6 IPv6 */
    uint32_t socktype; /* 0 unspecified, 1 stream, 2 datagram */
    uint32_t protocol; /* 0 unspecified, 6 TCP, 17 UDP */
} clbb_dns_hints_v1;
typedef struct {
    uint32_t family, socktype, protocol;
    uint16_t port; /* host order */
    uint16_t reserved;
    uint32_t scope_id, flowinfo;
    uint8_t address[16]; /* network-order bytes; first 4 for IPv4 */
} clbb_dns_record_v1;
typedef struct {
    uint32_t version, count, canonical_len;
    int32_t system_errno; /* Linux/Android errno, only on EAI_SYSTEM */
    uint8_t canonical[CLBB_DNS_CANON_CAPACITY]; /* NUL terminated */
    clbb_dns_record_v1 records[CLBB_DNS_MAX_RECORDS];
} clbb_dns_response_v1;

/* node/service are nullable NUL-terminated strings. hints is required.
 * response is required and reset on failure; count=0, canonical_len=0.
 * Buffers must be aligned, non-overlapping and valid for the whole call.
 * Success never silently truncates. No memory ownership passes to the caller.
 * Blocking call; host OS timeout/cancellation policies apply. */
int32_t clbb_dns_resolve_v1(const char *node, const char *service,
    const clbb_dns_hints_v1 *hints, clbb_dns_response_v1 *response);
#ifdef __cplusplus
}
#endif
#endif
