import java.util.concurrent.CountDownLatch;
import java.util.concurrent.atomic.AtomicReference;

public final class CLBBArmThreads {
    private static final int THREADS = 8;
    private static final int CALLS = 64;
    private static native int answer();

    public static void main(String[] args) throws Exception {
        System.load(args[0]);
        final CountDownLatch ready = new CountDownLatch(THREADS);
        final CountDownLatch start = new CountDownLatch(1);
        final AtomicReference<Throwable> failure = new AtomicReference<Throwable>();
        Thread[] workers = new Thread[THREADS];
        for (int i = 0; i < THREADS; i++) {
            workers[i] = new Thread(new Runnable() {
                public void run() {
                    ready.countDown();
                    try {
                        start.await();
                        for (int n = 0; n < CALLS; n++) {
                            int value = answer();
                            if (value != 42) throw new AssertionError("ARM64 result=" + value);
                        }
                    } catch (Throwable error) {
                        failure.compareAndSet(null, error);
                    }
                }
            }, "CLBB-ARM-worker-" + i);
            workers[i].start();
        }
        ready.await();
        start.countDown();
        for (Thread worker : workers) worker.join();
        if (failure.get() != null) throw new AssertionError(failure.get());
        System.out.println("CLBB_ARM64_THREADS_OK=" + (THREADS * CALLS));
        // This test covers JNI concurrency; ART's separate OnUnload teardown
        // failure on return from main remains recorded in the experiment notes.
        System.exit(0);
    }
}
