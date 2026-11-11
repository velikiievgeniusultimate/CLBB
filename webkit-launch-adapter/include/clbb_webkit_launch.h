#ifndef CLBB_WEBKIT_LAUNCH_H
#define CLBB_WEBKIT_LAUNCH_H
/* LD_PRELOAD adapter; no callable configuration API or transferred ownership.
 * execv/execve/execvp/posix_spawn/posix_spawnp retain their libc C ABIs.
 * Unset/empty variable disables relocation. Nonempty directory must be absolute.
 * Only the three exact absolute helper paths under CLBB_WEBKIT_COMPILED_DIR
 * are eligible; argv, envp, file actions and attributes are passed unchanged. */
#define CLBB_WEBKIT_PROCESS_DIR_ENV "CLBB_WEBKIT_PROCESS_DIR"
#define CLBB_WEBKIT_COMPILED_DIR "/usr/lib/webkitgtk-6.0"
#endif
