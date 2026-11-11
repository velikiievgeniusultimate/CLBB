public final class CLBBArmAttachExit {
    private static native int check();
    private static native int javaOwnedProbe();
    private static native boolean stillAttached();

    public static void main(String[] args) throws Exception {
        System.load(args[0]);
        int result = check();
        if (result != 15) {
            System.err.println("CLBB_ARM64_ATTACH_EXIT_FAILED=" + result);
            System.exit(1);
        }
        final int[] javaResult = new int[1];
        final Throwable[] failure = new Throwable[1];
        Thread javaThread = new Thread(new Runnable() {
            @Override public void run() {
                try {
                    int probe = javaOwnedProbe();
                    if (probe != 31 || !stillAttached())
                        throw new AssertionError("Java-owned attachment: " + probe);
                    /* Return to Java between native entries, then verify again. */
                    Thread.yield();
                    if (!stillAttached())
                        throw new AssertionError("Java-owned attachment was detached");
                    javaResult[0] = 16;
                } catch (Throwable problem) {
                    failure[0] = problem;
                }
            }
        }, "CLBB-Java-owned");
        javaThread.start();
        javaThread.join();
        if (failure[0] != null)
            throw new AssertionError("Java-owned thread failed", failure[0]);
        if (javaResult[0] != 16 || !stillAttached())
            throw new AssertionError("Parent attachment was lost");
        /* Guest joins use QEMU's child-tid wake; also allow host TLS exit
         * checks to run before reporting success. No fatal checks are changed. */
        Thread.sleep(100);
        result |= javaResult[0];
        System.out.println("CLBB_ARM64_ATTACH_EXIT_OK=" + result);
        System.exit(0);
    }
}
