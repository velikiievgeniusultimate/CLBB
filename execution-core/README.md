# CLBB execution core — first independent boundary

> Current status (2026-11-09): QEMU is the active backend. ETA experiments and
> their historical plans below are retired and stored outside this checkout;
> see [archive index](../ARCHIVES.md). This directory remains a required runtime
> Rust component and has not been removed. Old ETA relative paths below refer
> to archived history, not active dependencies.

The implemented path is:

```text
library loader / JNI trampoline / activity callbacks
  → GuestCore / GuestCpu
  → Rust execution ABI v1
  → immutable QEMU adapter
  → existing nb-qemu execution
```

This is an actual installed call path, not just a proposed interface. QEMU is
currently the **only production backend**. The independent Rust reference
interpreter in `../eta-core/` now executes a small A64 subset in synthetic
programs. There is no game-capable independent JIT, block-level hybrid fallback,
live backend switch or Windows build yet.

`src/lib.rs` + `src/abi.rs` form an independent no_std Rust object. They import
neither QEMU headers/symbols nor platform APIs, allocator or standard library.
The native bridge links this object separately from Rust PAC arithmetic. The
Makefile rejects unresolved Rust object symbols. `clbb_execution.h` is the
64-bit-host C ABI contract; pointer-sized words use the host C ABI (including
future Windows), and the full 64-bit return path is distinct from word returns.
The Rust symbols are hidden in the final native bridge.

The descriptor owns no guest state: it is immutable and must outlive CPU handles.
It carries six operations: initialization, symbol lookup, acquire/release CPU,
word call, full u64 call. Each invocation forwards eight register arguments and
the original host stack pointer/length. Interface errors have separate status
codes, preserving valid negative/zero guest results. Invalid ABI or call shapes
fail before execution; the embedding treats internal contract violations as
fatal rather than returning a plausible guest value or retrying side effects.

The embedding must supply valid aligned host pointers, disjoint output storage,
live strings/buffers and non-unwinding callbacks. Null checks cannot prove
arbitrary host pointer validity. Rust does not reinterpret CPUARMState or
access guest memory. No Rust mutable global, allocation, mutex or lazy backend
selection is introduced. Re-entry is allowed. A selected backend stays fixed
for a CPU's lifetime, including thread-local destruction.

The QEMU adapter borrows an already current QEMU CPU or creates a managed one.
`GuestCpu` retains original per-thread caching and deletes only managed CPUs.
QemuCpu remains a private source alias to GuestCpu, not a second owner/TLS.
Initialization/symbol lookup in the loader and JavaBridge now use GuestCore.

## Принятое направление — 2026-11-08

По решению пользователя строим собственный запускатор Mobile Legends для
Linux и Windows. Основной новый код — Rust; встроенный или отдельный ассемблер
применяем там, где требуется точное управление регистрами, стеком и переходом
к сгенерированному коду. Написание всего движка на ассемблере не является целью.
Слой `unsafe` должен быть ограничен и иметь явно описанные требования;
безопасность Rust не распространяется автоматически на оставшийся C/FFI.

QEMU сохраняется как рабочий движок и независимый эталон при развитии нашего
ядра. Конечная цель — возможность заменить его собственной реализацией,
а не обязательный буквальный перенос всего исходного дерева QEMU на Rust.
Код игры/Unity остаётся ARM64-кодом; его переводит движок исполнения.
Сначала доказываем эквивалентность и штатную работу, затем отдельно оптимизируем.
Каждый подключаемый модуль также проверяем на ухудшение производительности;
сам язык реализации не считается доказательством ускорения.

### Реализовано

- Отдельный Rust execution-core и ABI v1 с адаптером QEMU.
- Рабочие вызовы загрузчика/JNI через GuestCore/GuestCpu.
- Независимые проверки без QEMU, ошибки контракта, повторный вход и потоки.
- Проверки настоящих ARM-инструкций и запуск игры до лобби; записи находятся
  в `../logs/execution-core-installed-proof.json` и
  `../logs/startup-execution-core-01/`. Этот запуск не проверял ввод.
- Отдельный Rust PAC-модуль в `../translator-rust/`.
- Собственный безопасный Rust-интерпретатор в `../eta-core/`: подмножество A64,
  270 программ / 34560 дифференциальных исполнений против QEMU. В игру не подключён.

### План и оставшаяся работа

1. **Начато; первая версия готова в `../eta-core/`.** Расширять собственную
   модель состояния ARM64 и небольшой эталонный исполнитель на
   Rust для тестовых программ: регистры, флаги, арифметика, переходы и память.
   Сравнивать состояния/ошибки с QEMU на одинаковых входных данных; это ещё
   не попытка запустить всю игру своим движком.
2. Параллельно выделить контракт гостевой памяти и системный слой: права
   отображений, CPU/потоки/TLS, системные вызовы, ошибки памяти и сигналы.
   Linux-зависимости держать в Linux-адаптере; Windows-адаптер реализуется
   отдельно. Переносимость интерфейса не равна проверенной Windows-сборке.
3. Добавить генерацию x86-64-кода и кеш блоков, сохраняя результаты эталонного
   исполнителя. Ассемблерные переходники используют явный контракт регистров,
   стека и сохраняемого состояния.
4. Подключать проверенные участки к игре только после согласования памяти,
   JNI, потоков и исключений. Для совместного исполнения с QEMU заранее
   определить точки передачи управления и полное состояние CPU/FP, правила
   атомиков, изменения кода и инвалидирования кеша. Повтор вызова после уже
   совершённых побочных эффектов не является безопасным fallback.
5. После функциональной проверки сопоставимых запусков оптимизировать горячие
   участки. Убирать QEMU только по мере доказанной полноты нашей реализации.

## Still coupled to QEMU

- Guest allocation, pointer translation and string/memory access (QemuMemory).
- Syscall/SVC handlers and signal handling, including Linux-specific behavior.
- JNI/native thunk code that currently uses QEMU CPU layouts.
- ELF/linker, translation cache, decoding, TCG, atomics and fault semantics.

These are explicit next migration boundaries. The descriptor alone does not
make the whole game portable or permit replacing QEMU with an unrelated CPU
engine. A later memory/platform contract must first define shared mappings,
thread/TLS lifetime, host/guest faults and callbacks. Instruction-level mixed
execution additionally needs exact registers/flags/FP state and invalidation.

## Validation

```sh
bash execution-core/check.sh
bash smoke/run-checks.sh
```

The standalone contract links only the Rust execution object, GuestCpu and a
mock provider — **no QEMU library**. Both normal and ASan/UBSan C++ harnesses
exercise version/size/missing callbacks, invalid requests, acquire failure,
invalid ownership, unchanged error outputs, lookup misses, backend init errors,
eight register arguments, stack payload, negative and high-bit 64-bit results,
recursive entry, eight managed threads and one borrowed thread. Exactly eight
CPU allocations are released; the borrowed CPU is never deleted. Sanitizers
instrument the C++ boundary/harness, not Rust machine code.

Installed ARM fixtures/smoke exercise real QEMU guest ABI, signals, threads,
SMC, PAC/AUT and vector registers. Passive real-game runs are saved separately;
process survival alone is not lobby readiness or verified input responsiveness.
No performance improvement is claimed for this architectural boundary.

The core is GPL-2.0-or-later. Adapted GuestCpu files preserve nb-qemu authorship
and license notices. Existing third-party licenses remain applicable.


## Independent runner and first JIT (2026-11-08)

The separate MIT `../eta-core` now includes baseline scalar/vector FP, selected
integer SIMD, widening/high-half multiply, TLS and explicit SVC/HLT exits.
`../eta-host-adapter` executes this core through a full 816-byte StateV1,
normal-memory callbacks and structured exits. Unlike the production V1 facade,
it actually executes instructions without linking QEMU. It has standalone
state/layout, precise-fault, explicit-resume and distinct-context reentry tests.
It does not provide syscalls, JNI/GL services, shared-memory atomics or signals.

`../eta-jit` independently emits bounded x86-64 blocks for a narrow integer
subset. Native tests use the actual adapter StateV1 and compare 29712 calls
under SysV/Win64 calling conventions on Linux against the reference interpreter.
There is no game dispatch/cache or full Windows runtime yet. These experiments
are not registered as a live production backend and do not establish that the
first-frame-to-lobby interval runs independently. See their READMEs for evidence.
