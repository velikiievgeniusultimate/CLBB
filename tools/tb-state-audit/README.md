# AArch64 TB state fast path

The startup profile attributed CPU time to `helper_lookup_tb_ptr` and the
out-of-line `cpu_get_tb_cpu_state`. The ARM header now inlines only the A64
user-mode state getter, keeping every dynamic BTI/BTYPE and single-step bit.
A32, system-mode and debug hflags validation use `arm_get_tb_cpu_state`, whose
complete original body is unchanged. The lookup table, invalidation, ownership,
trace state and guest exception behavior are unchanged.

`check.sh` includes the real production QEMU header and invokes the complete
original getter in a real bridge library via `dlsym`. It compares 1,000,512
random/structured states (BTI present/absent, BTYPE, SS-active/PSTATE-SS, PC and
A32 fallback), with UBSan. This does not create a VM or access game memory.

`lookup.c` benchmarks the actual exported `helper_lookup_tb_ptr` of two bridge
libraries against populated valid 64-target TB jump caches. The baseline and
candidate gave median approximately2.89ns and2.44ns per hit on this machine;
this is a microbenchmark, not an18% whole-startup claim. Every returned code
pointer and checksum is checked. Fixtures use the production CPU/TB layouts.

Run `tools/tb-state-audit/check.sh [bridge.so]`. The target bridge must expose
`arm_get_tb_cpu_state` (new) or the previous `cpu_get_tb_cpu_state` (old).
The helper body remains the source of truth for generic/debug states.
