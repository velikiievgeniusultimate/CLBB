public class CLBBArmSignal {
    private static native int check();
    public static void main(String[] args) {
        System.load(args[0]);
        int result = check();
        if (result != 11) throw new AssertionError("Guest signal: " + result);
        System.out.println("CLBB_ARM64_SIGNAL_OK=" + result);
        System.exit(0);
    }
}
