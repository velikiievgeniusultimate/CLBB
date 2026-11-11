public final class CLBBArmLoadFailure {
    public static void main(String[] args) {
        if (args.length != 1) {
            throw new IllegalArgumentException("Expected the ARM64 test library path");
        }

        try {
            System.load(args[0]);
        } catch (UnsatisfiedLinkError expected) {
            String diagnostic = expected.getMessage();
            if (diagnostic == null || diagnostic.trim().isEmpty()
                    || !diagnostic.contains("clbb_intentionally_missing_dependency")) {
                throw new AssertionError("Wrong guest loader diagnostic: " + diagnostic, expected);
            }
            System.out.println("CLBB_ARM64_LOAD_FAILURE_OK=1");
            System.exit(0);
            return;
        }

        throw new AssertionError("ARM64 library with a missing strong symbol loaded successfully");
    }
}
