public final class CLBBArmLseContract {
    private static native int check();
    public static void main(String[] args) {
        System.load(args[0]);
        int result = check();
        if (result != 0) throw new AssertionError("ARM LSE contract line=" + result);
        System.out.println("CLBB_ARM64_LSE_CONTRACT_OK=1");
        System.exit(0);
    }
}
