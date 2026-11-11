public class CLBBArmSignalNested {
    private static native int check();
    public static void main(String[] args) {
        System.load(args[0]);
        int result = check();
        if (result != 30) {
            System.err.println("CLBB_ARM64_SIGNAL_NESTED_FAILED=" + result);
            System.exit(1);
        }
        System.out.println("CLBB_ARM64_SIGNAL_NESTED_OK=" + result);
        System.exit(0);
    }
}
