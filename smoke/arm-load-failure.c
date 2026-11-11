/* A data relocation must resolve during dlopen even with RTLD_LAZY. Keeping
 * this symbol strong makes loader failure mandatory before any guest call. */
extern int clbb_intentionally_missing_dependency;

__attribute__((used, visibility("default")))
int *const clbb_load_failure_reference = &clbb_intentionally_missing_dependency;
