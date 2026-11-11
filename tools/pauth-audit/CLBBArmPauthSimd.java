public class CLBBArmPauthSimd {
    private static native int check();
    public static void main(String[] args) {
        System.load(args[0]);
        int result = check();
        if (result != 0) {
            System.err.println("CLBB_ARM64_PAUTH_SIMD_FAILED=" + result);
            System.exit(1);
        }
        System.out.println("CLBB_ARM64_PAUTH_SIMD_OK=all32qregs_roundtrip_and_corrupt_rejected");
        System.exit(0);
    }
}
