public final class CLBBArmSmoke {
    private static native int answer();
    public static void main(String[] args) {
        System.load(args[0]);
        int value = answer();
        if (value != 42) throw new AssertionError("Wrong ARM64 result: " + value);
        System.out.println("CLBB_ARM64_BRIDGE_OK=" + value);
    }
}
