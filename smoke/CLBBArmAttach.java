public class CLBBArmAttach {
    private static native int check();
    public static void main(String[] args) {
        System.load(args[0]);
        int result = check();
        if (result != 127) {
            System.err.println("CLBB_ARM64_ATTACH_FAILED=" + result);
            System.exit(1);
        }
        System.out.println("CLBB_ARM64_ATTACH_OK=" + result);
        System.exit(0);
    }
}
