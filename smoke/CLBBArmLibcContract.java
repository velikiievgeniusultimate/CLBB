public final class CLBBArmLibcContract {
    private static native int check();
    public static void main(String[] args) {
        System.load(args[0]);
        int result = check();
        if (result != 0) throw new AssertionError("libc contract line=" + result);
        System.out.println("CLBB_ARM64_LIBC_CONTRACT_OK=1");
        // Keep the unrelated ART JNI_OnUnload shutdown path out of this fixture.
        System.exit(0);
    }
}
