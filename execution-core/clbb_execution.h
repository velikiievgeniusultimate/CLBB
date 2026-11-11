/* SPDX-License-Identifier: GPL-2.0-or-later */
#ifndef CLBB_EXECUTION_H
#define CLBB_EXECUTION_H
#include <stddef.h>
#include <stdint.h>
#if INTPTR_MAX != INT64_MAX
#error CLBB execution ABI v1 requires a 64-bit host
#endif
#ifdef __cplusplus
extern "C" {
#endif
/* Host ABI, version 1: x86_64 Linux and future Windows. Guest words are 64 bit.
 * Embedding provides immutable, aligned, live descriptors and request buffers.
 * Descriptor is fixed for a CPU's lifetime; live backend switching is absent.
 * Engine errors are separate from guest return values. No panic/unwind across
 * this boundary, and callbacks must not retain request/stack pointers. */
typedef struct {
    intptr_t address;
    intptr_t args[8];
    uint8_t *stack;
    int32_t stack_size;
} ClbbExecutionCallV1;
typedef struct {
    uint32_t abi, size;
    int32_t (*initialize)(const char *, const char *);
    intptr_t (*lookup)(const char *);
    void *(*acquire)(uint32_t *owned);
    void (*release)(void *);
    intptr_t (*call_word)(void *, const ClbbExecutionCallV1 *);
    uint64_t (*call_u64)(void *, const ClbbExecutionCallV1 *);
} ClbbExecutionBackendV1;
#define CLBB_EXECUTION_ABI_V1 1u
/* These symbols are linked locally in the native bridge. */
#if defined(__GNUC__)
#define CLBB_EXEC_LOCAL __attribute__((visibility("hidden")))
#else
#define CLBB_EXEC_LOCAL
#endif
CLBB_EXEC_LOCAL int32_t clbb_execution_initialize_v1(const ClbbExecutionBackendV1 *,const char *,const char *,int32_t *);
CLBB_EXEC_LOCAL int32_t clbb_execution_lookup_v1(const ClbbExecutionBackendV1 *,const char *,intptr_t *);
CLBB_EXEC_LOCAL int32_t clbb_execution_acquire_v1(const ClbbExecutionBackendV1 *,void **,uint32_t *);
CLBB_EXEC_LOCAL int32_t clbb_execution_release_v1(const ClbbExecutionBackendV1 *,void *,uint32_t);
CLBB_EXEC_LOCAL int32_t clbb_execution_call_word_v1(const ClbbExecutionBackendV1 *,void *,const ClbbExecutionCallV1 *,intptr_t *);
CLBB_EXEC_LOCAL int32_t clbb_execution_call_u64_v1(const ClbbExecutionBackendV1 *,void *,const ClbbExecutionCallV1 *,uint64_t *);
#ifdef __cplusplus
}
#endif
#endif
