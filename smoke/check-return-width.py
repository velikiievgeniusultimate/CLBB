#!/usr/bin/env python3
"""Exercise generated ARM64 return thunks, including asset-sized results."""
from pathlib import Path
import subprocess
root = Path(__file__).resolve().parent.parent
out = root / 'smoke/return-width'
(out / 'log/log.h').write_text('#define LOG_ALWAYS_FATAL(...) __builtin_trap()\n')
(out / 'QemuInclude.h').write_text('''#include <stdint.h>
typedef uint64_t abi_ulong;
typedef struct { uint64_t xregs[32]; uint64_t pc; struct { __uint128_t zregs[32]; } vfp; } CPUARMState;
static inline void *g2h(uint64_t p) { return (void *)(uintptr_t)p; }
static inline uint64_t h2g_nocheck(void *p) { return (uintptr_t)p; }
''')
(out / 'api.h').write_text('''#include <stdint.h>
#include <sys/types.h>
typedef float GLclampf;
typedef unsigned int EGLBoolean;
int ret_i32(void); int64_t ret_i64(void); off_t ret_offset(void);
uint8_t ret_u8(void); int16_t ret_i16(void); float ret_f32(void);
double ret_f64(void); GLclampf ret_clampf(void); EGLBoolean ret_bool(void);
''')
(out / 'api.def').write_text('''%include "api.h"
int ret_i32 (void)
int64_t ret_i64 (void)
off_t ret_offset (void)
uint8_t ret_u8 (void)
int16_t ret_i16 (void)
float ret_f32 (void)
double ret_f64 (void)
GLclampf ret_clampf (void)
EGLBoolean ret_bool (void)
''')
code = subprocess.check_output(['python', str(root/'src/nb-qemu/genlib.py'), str(out/'api.def')], text=True)
(out / 'generated.c').write_text(code)
(out / 'check.c').write_text('''#include <assert.h>
#include <string.h>
#include "generated.c"
int ret_i32(void) { return -1234567; }
int64_t ret_i64(void) { return INT64_C(0x12345678abcdef01); }
off_t ret_offset(void) { return (off_t)29155691; }
uint8_t ret_u8(void) { return 231; }
int16_t ret_i16(void) { return -1234; }
float ret_f32(void) { return 1.25f; }
double ret_f64(void) { return 2.75; }
GLclampf ret_clampf(void) { return .625f; }
EGLBoolean ret_bool(void) { return 0x1234; }
int main(void) {
 CPUARMState env = {0}; float f; double d;
 nb_handle_ret_i32(&env); assert(env.xregs[0] == (uint32_t)-1234567);
 nb_handle_ret_i64(&env); assert(env.xregs[0] == UINT64_C(0x12345678abcdef01));
 nb_handle_ret_offset(&env); assert(env.xregs[0] == 29155691);
 nb_handle_ret_u8(&env); assert(env.xregs[0] == 231);
 nb_handle_ret_i16(&env); assert((int16_t)env.xregs[0] == -1234);
 nb_handle_ret_f32(&env); memcpy(&f, &env.vfp.zregs[0], sizeof f); assert(f == 1.25f);
 nb_handle_ret_f64(&env); memcpy(&d, &env.vfp.zregs[0], sizeof d); assert(d == 2.75);
 nb_handle_ret_clampf(&env); memcpy(&f, &env.vfp.zregs[0], sizeof f); assert(f == .625f);
 nb_handle_ret_bool(&env); assert(env.xregs[0] == 0x1234);
 return 0;
}
''')
subprocess.run(['cc', '-O2', '-fno-strict-aliasing', '-fsanitize=address,undefined', '-I'+str(out), str(out/'check.c'), '-o', str(out/'check')], check=True)
subprocess.run([str(out/'check')], check=True)
print('CLBB_ARM64_RETURN_WIDTH_OK=9')
