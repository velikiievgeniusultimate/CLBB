public final class CLBBArmFstatatContract {
    private static native long run(int scenario, int operation);
    public static void main(String[] args) {
        int scenario = Integer.parseInt(args[1]);
        int operation = args.length > 2 ? Integer.parseInt(args[2]) : 79;
        System.load(args[0]);
        long result = run(scenario, operation);
        long expected = (scenario == 0 || (scenario >= 4 && scenario <= 6)) ? 0 : -14;
        System.out.println("CLBB_FSTATAT_TESTED_SYSCALL=" + operation);
        System.out.println("CLBB_FSTATAT_RESULT=" + result);
        if (result != expected) {
            System.out.println("CLBB_FSTATAT_CONTRACT_OK=0");
            System.exit(1);
        }
        System.out.println("CLBB_FSTATAT_CONTRACT_OK=1");
        System.exit(0);
    }
}
