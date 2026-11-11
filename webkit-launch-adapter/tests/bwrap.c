#define _GNU_SOURCE
#include <assert.h>
#include <errno.h>
#include <spawn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#ifdef CAPTURE_BACKEND
/* This second preload captures calls after the adapter. No real bwrap, WebKit
 * process, or sandbox is executed by this test. */
static int capture(const char *path, char *const argv[], char *const envp[],
                   const void *actions, const void *attrs) {
    assert(!strcmp(path, "/usr/bin/bwrap"));
    assert(!strcmp(argv[0], "unchanged argv zero"));
    assert(!strcmp(argv[1], "--args") && !strcmp(argv[2], "51"));
    assert(!strcmp(argv[3], "/usr/lib/webkitgtk-6.0/WebKitGPUProcess"));
    assert(!strcmp(argv[4], "--"));
    char expected[4096];
    snprintf(expected, sizeof expected, "%s/WebKitWebProcess", getenv("CLBB_WEBKIT_PROCESS_DIR"));
    assert(!strcmp(argv[5], expected));
    assert(!strcmp(argv[6], "unchanged argument") && argv[7] == NULL);
    char byte; assert(read(51, &byte, 1) == 1 && byte == 'F');
    if (envp) assert(!strcmp(envp[0], "EXPLICIT_ENV=unchanged") && envp[1] == NULL);
    if (actions) assert(actions == (void *)0x1230 && attrs == (void *)0x4560);
    errno = EACCES;
    return -1;
}
int execv(const char *p, char *const a[]) { return capture(p,a,NULL,NULL,NULL); }
int execvp(const char *p, char *const a[]) { return capture(p,a,NULL,NULL,NULL); }
int execve(const char *p, char *const a[], char *const e[]) { return capture(p,a,e,NULL,NULL); }
int posix_spawn(pid_t *pid, const char *p, const posix_spawn_file_actions_t *f,
    const posix_spawnattr_t *at, char *const a[], char *const e[]) {
    (void)pid; capture(p,a,e,f,at); return EACCES;
}
int posix_spawnp(pid_t *pid, const char *p, const posix_spawn_file_actions_t *f,
    const posix_spawnattr_t *at, char *const a[], char *const e[]) {
    return posix_spawn(pid,p,f,at,a,e);
}
#else
int main(void) {
    int fds[2]; assert(pipe(fds) == 0); assert(dup2(fds[0],51) == 51);
    char *args[] = {"unchanged argv zero", "--args", "51",
        "/usr/lib/webkitgtk-6.0/WebKitGPUProcess", "--",
        "/usr/lib/webkitgtk-6.0/WebKitWebProcess", "unchanged argument", NULL};
    char *env[] = {"EXPLICIT_ENV=unchanged", NULL};
    pid_t pid;
    for (int api=0; api<5; ++api) {
        assert(write(fds[1],"F",1) == 1);
        int rc;
        if(api==0) rc=execv("/usr/bin/bwrap",args);
        else if(api==1) rc=execve("/usr/bin/bwrap",args,env);
        else if(api==2) rc=execvp("/usr/bin/bwrap",args);
        else if(api==3) rc=posix_spawn(&pid,"/usr/bin/bwrap",(void*)0x1230,(void*)0x4560,args,env);
        else rc=posix_spawnp(&pid,"/usr/bin/bwrap",(void*)0x1230,(void*)0x4560,args,env);
        assert(api<3 ? rc==-1 && errno==EACCES : rc==EACCES);
        assert(!strcmp(args[5],"/usr/lib/webkitgtk-6.0/WebKitWebProcess"));
    }
    char *many[2049]; for(int i=0;i<2048;++i) many[i]="x"; many[2048]=NULL;
    assert(posix_spawn(&pid,"/usr/bin/bwrap",NULL,NULL,many,env)==E2BIG);
    char *four[]={"bwrap","--",
        "/usr/lib/webkitgtk-6.0/WebKitWebProcess","/usr/lib/webkitgtk-6.0/WebKitWebProcess",
        "/usr/lib/webkitgtk-6.0/WebKitWebProcess","/usr/lib/webkitgtk-6.0/WebKitWebProcess",NULL};
    assert(posix_spawn(&pid,"/usr/bin/bwrap",NULL,NULL,four,env)==E2BIG);
    puts("CLBB_WEBKIT_BWRAP_OK=7");
}
#endif
