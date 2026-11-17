import java.util.concurrent.CountDownLatch;
import java.util.concurrent.atomic.AtomicReference;

public final class CLBBArmIndirect {
    private static native boolean check(int lane);
    public static void main(String[] args) throws Exception {
        System.load(args[0]);
        int count = 8;
        CountDownLatch ready = new CountDownLatch(count);
        CountDownLatch start = new CountDownLatch(1);
        AtomicReference<Throwable> failure = new AtomicReference<>();
        Thread[] workers = new Thread[count];
        for (int i = 0; i < count; i++) {
            final int lane = i;
            workers[i] = new Thread(new Runnable() {
                @Override public void run() {
                    ready.countDown();
                    try {
                        start.await();
                        if (!check(lane)) throw new AssertionError("branch target " + lane);
                    } catch (Throwable ex) {
                        failure.compareAndSet(null, ex);
                    }
                }
            });
            workers[i].start();
        }
        ready.await();
        start.countDown();
        for (Thread worker : workers) worker.join();
        if (failure.get() != null) throw new AssertionError(failure.get());
        System.out.println("CLBB_ARM64_INDIRECT_OK=" + count);
        System.exit(0);
    }
}
