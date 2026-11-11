/* SPDX-License-Identifier: GPL-2.0-or-later
 * QEMU-dependent experimental boundary; the independent Rust core remains MIT.
 */
#ifndef CLBB_QEMU_STATE_TRANSFER_H
#define CLBB_QEMU_STATE_TRANSFER_H
#include "qemu/osdep.h"
#include "cpu.h"
#include "clbb_arm64_host.h"
/* The caller exclusively owns a stopped little-endian EL0 AArch64 env from a live ARMCPU.
 * State buffers are aligned, valid and disjoint from env. No callbacks, guest
 * execution, CPU ownership changes or pointers retained by these functions.
 * Export sets the V1 header. Import validates all fields before mutation.
 * Only V0..V31 low128 bits transfer; SVE/predicates/exclusive state do not.
 */
int32_t clbb_qemu_state_export_v1(CPUARMState *, ClbbA64StateV1 *);
int32_t clbb_qemu_state_import_v1(CPUARMState *, const ClbbA64StateV1 *);
#endif
