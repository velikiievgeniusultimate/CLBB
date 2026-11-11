# Linux WebKit helper relocation

An isolated Rust `LD_PRELOAD` library, with no Cargo dependencies. It rewrites
only these exact executable paths when `CLBB_WEBKIT_PROCESS_DIR` is nonempty:

* `/usr/lib/webkitgtk-6.0/WebKitNetworkProcess`
* `/usr/lib/webkitgtk-6.0/WebKitWebProcess`
* `/usr/lib/webkitgtk-6.0/WebKitGPUProcess`

The replacement is the configured absolute directory plus the same basename.
All other executable paths, including relative names, are forwarded unchanged.
For the exact executable `/usr/bin/bwrap`, only matching helper arguments after
the first `--` are rewritten. Options before `--`, including `--args` and its FD,
are never inspected or changed. Original argv strings/pointers are not mutated.
No sandbox, TLS, certificate or WebKit protection is disabled.

The local GLib imports `execv`, `execve`, `execvp` and `fork`; the local WebKit
library imports `posix_spawn` and `posix_spawnp`. These five launch functions are
interposed with their libc ABI. File actions, spawn attributes, argv (except the
specified bwrap operands), and envp are passed through unchanged. An unset or
empty directory disables relocation. Relative directories fail matched launches
with EINVAL; long paths fail with ENAMETOOLONG. bwrap argv is bounded to 2047
arguments plus NULL and at most three rewritten arguments; overflow is E2BIG.
exec APIs return -1/set errno; posix_spawn APIs return the error number directly.

Function pointers resolve through `dlsym(RTLD_NEXT)` in `.init_array`, before
forking. The launch hooks use only fixed stack buffers, atomic pointer loads,
libc `getenv`, byte copies, and the real launch function. They perform no Rust
heap allocations, logging, locks, lazy initialization or post-fork `dlsym`.
Environment configuration must not be mutated concurrently with spawning.

Build and fake-helper regression (no real WebKit/bwrap/GUI is launched):

```bash
cargo build --manifest-path webkit-launch-adapter/Cargo.toml --offline --locked --release
bash webkit-launch-adapter/test.sh
```

The artifact is `target/release/libclbb_webkit_launch_adapter.so`. Configure
`CLBB_WEBKIT_PROCESS_DIR` and prepend the absolute artifact path to host
`LD_PRELOAD`. Keep this host preload out of the ARM guest linker environment.
The surrounding WebKit runtime must expose the configured helper directory to
its normal sandbox; this adapter does not modify its filesystem permissions.
An installed release WebKit here ignores `WEBKIT_EXEC_PATH`, so this workaround
does not depend on that developer-only override.

The regression verifies all three fake helpers through all five imported libc
APIs and GLib spawning, explicit envp, argv[0], an inherited descriptor, spawn
file actions and a signal-mask attribute; an unrelated executable is unchanged.
A second capture-only preload checks bwrap argv/env/actions/attrs/FD preservation
and bounded failure paths. No files are installed into the experiment prefix or
system directories.
