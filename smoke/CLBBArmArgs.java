public class CLBBArmArgs {
    private static native int check(String a, String b, int c, String d, int e,
                                    String f, int g, String h, String i,
                                    String j, String k);
    public static void main(String[] args) {
        System.load(args[0]);
        String value = "CLBB JNI arguments";
        int result = check(value, value, 17, value, 31, value, 47,
                           value, value, value, value);
        if (result != 127) throw new AssertionError("JNI stack ABI: " + result);
        System.out.println("CLBB_ARM64_JNI_STACK_OK=" + result);
        System.exit(0);
    }
}
