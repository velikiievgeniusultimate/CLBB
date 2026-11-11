public class CLBBArmVarargs {
    private static native int check();
    public static int callback(int a, double x, int b, double y,
            int c, double z, int d, double u, int e, double v,
            int f, double w, int g, double p, int h, double q,
            int i, double r, int j, double s) {
        int mask = 0;
        if (a == 11 && x == 1.25) mask |= 1;
        if (b == 22 && y == 2.25) mask |= 2;
        if (c == 33 && z == 3.25) mask |= 4;
        if (d == 44 && u == 4.25) mask |= 8;
        if (e == 55 && v == 5.25) mask |= 16;
        if (f == 66 && w == 6.25) mask |= 32;
        if (g == 77 && p == 7.25) mask |= 64;
        if (h == 88 && q == 8.25) mask |= 128;
        if (i == 99 && r == 9.25) mask |= 256;
        if (j == 110 && s == 10.25) mask |= 512;
        return mask;
    }
    public static void main(String[] args) {
        System.load(args[0]);
        int result = check();
        if (result != 1023) {
            System.err.println("CLBB_ARM64_VARARGS_FAILED=" + result);
            System.exit(1);
        }
        System.out.println("CLBB_ARM64_VARARGS_OK=" + result);
        System.exit(0);
    }
}
