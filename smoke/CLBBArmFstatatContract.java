public final class CLBBArmFstatatContract {
    private static native long run(int scenario);
    public static void main(String[] args) {
        int scenario = Integer.parseInt(args[1]);
        System.load(args[0]);
        long result = run(scenario);
        long expected = (scenario == 0 || scenario >= 4) ? 0 : -14; // Raw -EFAULT.
        System.out.println("CLBB_FSTATAT_RESULT=" + result);
        if (result != expected) {
            System.out.println("CLBB_FSTATAT_CONTRACT_OK=0");
            System.exit(1);
        }
        System.out.println("CLBB_FSTATAT_CONTRACT_OK=1");
        System.exit(0);
    }
}
