public class CLBBArmYield {
    private static native int check();
    public static void main(String[] args) {
        System.load(args[0]);
        int result = check();
        if (result != 559) {
            System.err.println("CLBB_ARM64_YIELD_FAILED=" + result);
            System.exit(1);
        }
        System.out.println("CLBB_ARM64_YIELD_OK=" + result);
        System.exit(0);
    }
}
