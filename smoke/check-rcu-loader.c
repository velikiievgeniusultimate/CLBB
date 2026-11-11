#include <dlfcn.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static void *bridge;
static void (*register_reader)(void);
static void (*unregister_reader)(void);
static void *load_bridge(void *path) {
    bridge=dlopen(path, RTLD_NOW | RTLD_LOCAL);
    if (!bridge) fprintf(stderr,"%s\n",dlerror());
    return NULL;
}
static void *churn(void *unused) {
    register_reader();
    register_reader();
    void *reader=dlsym(bridge,"rcu_reader");
    if (!reader) abort();
    void *bytes=malloc(4096); memset(bytes,0x35,4096); free(bytes);
    if ((size_t)unused & 1) { unregister_reader(); unregister_reader(); }
    return NULL;
}
int main(int argc,char **argv) {
    pthread_t thread;
    if (argc != 2 || pthread_create(&thread,NULL,load_bridge,argv[1])) return 1;
    pthread_join(thread,NULL);
    if (!bridge) return 2;
    void (*synchronize)(void)=dlsym(bridge,"synchronize_rcu");
    if (!synchronize) return 3;
    register_reader=dlsym(bridge,"rcu_register_thread");
    unregister_reader=dlsym(bridge,"rcu_unregister_thread");
    if (!register_reader || !unregister_reader) return 5;
    for (int i=0;i<128;++i) {
        if (pthread_create(&thread,NULL,churn,(void *)(size_t)i)) return 4;
        pthread_join(thread,NULL);
        synchronize();
    }
    puts("CLBB_RCU_LOADER_OK=128");
    return 0;
}
