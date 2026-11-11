/* SPDX-License-Identifier: MIT
 * Standalone independent A64 interpreter embedding. No QEMU/OS dependency.
 * All struct pointers must be non-null, aligned, readable/writable as stated,
 * live for the call, and disjoint from each other and callback memory buffers.
 * V1 descriptors/requests are immutable during a call. Callbacks may reenter
 * with DISTINCT state/request/exit buffers; overlapping active state/exit buffers
 * are rejected with CLBB_A64_ACTIVE_STATE. Immutable memory descriptors may be reused.
 * No pointer retained by adapter or callback. No exception/unwind across ABI.
 */
#ifndef CLBB_ARM64_HOST_H
#define CLBB_ARM64_HOST_H
#include <stddef.h>
#include <stdint.h>
#if INTPTR_MAX != INT64_MAX
#error A64 host adapter V1 requires a 64-bit host
#endif
#ifdef __cplusplus
extern "C" {
#endif
#define CLBB_A64_ABI_V1 1u
#define CLBB_A64_OK 0
#define CLBB_A64_BAD_ARGUMENT -1
#define CLBB_A64_BAD_ABI -2
#define CLBB_A64_BAD_STATE -3
#define CLBB_A64_INTERNAL_PANIC -4
#define CLBB_A64_ACTIVE_STATE -5
#define CLBB_A64_READ 1u
#define CLBB_A64_EXECUTE 2u
#define CLBB_A64_WRITE 3u
/* Memory callback result. A failed WRITE MUST leave the ENTIRE span unchanged.
 * Callbacks must not retain buffer pointers. READ fills every byte on success;
 * EXECUTE is distinct from READ permission, and no host pointer is guest PC.
 * Callback must synchronize mapping lifetimes and normal-memory accesses.
 * Atomic/MMIO/exclusive accesses are not modeled by this V1 interface.
 */
#define CLBB_A64_MEM_OK 0
#define CLBB_A64_UNMAPPED 1
#define CLBB_A64_PERMISSION 2
#define CLBB_A64_ALIGNMENT 3
#define CLBB_A64_CALLBACK_ERROR 4

typedef struct {
 uint32_t abi,size;
 uint64_t x[31];
 uint64_t sp,pc;
 uint64_t v[32][2]; /* low64,high64; architectural little-endian bits */
 uint32_t nzcv,fpcr,fpsr,reserved;
 uint64_t tpidr_el0,tpidrro_el0;
} ClbbA64StateV1;
typedef int32_t (*ClbbA64ReadV1)(void*,uint64_t,uint8_t*,uint32_t,uint32_t);
typedef int32_t (*ClbbA64WriteV1)(void*,uint64_t,const uint8_t*,uint32_t);
typedef struct {
 uint32_t abi,size;
 void *context;
 ClbbA64ReadV1 read;
 ClbbA64WriteV1 write;
} ClbbA64MemoryV1;
#define CLBB_A64_STOP_AT_RETURN 1u
typedef struct {
 uint32_t abi,size;
 uint64_t budget,return_pc;
 uint32_t flags,reserved;
} ClbbA64RequestV1;
#define CLBB_A64_EXIT_BUDGET 0u
#define CLBB_A64_EXIT_SVC 1u
#define CLBB_A64_EXIT_HALT 2u
#define CLBB_A64_EXIT_BREAKPOINT 3u
#define CLBB_A64_EXIT_MEMORY 4u
#define CLBB_A64_EXIT_UNSUPPORTED 5u
#define CLBB_A64_EXIT_RETURN 6u
/* steps = successfully completed instructions, not trap/fault attempts.
 * SVC/HLT/BRK and faults leave PC at the instruction. Caller explicitly decides
 * resume PC/return registers; adapter invokes no syscall or JNI service.
 */
typedef struct {
 uint32_t abi,size,reason,detail;
 uint64_t pc,address,steps;
 uint32_t access,reserved;
} ClbbA64ExitV1;
int32_t clbb_arm64_run_v1(ClbbA64StateV1*,const ClbbA64MemoryV1*,const ClbbA64RequestV1*,ClbbA64ExitV1*);
#ifdef __cplusplus
}
#endif
#endif
