static int do_recip_sqrt_estimate(int a)
{
    int estimate;

    assert(128 <= a && a < 512);
    estimate = 256 + clbb_rsqrt_estimate_minus_256[a - 128];
    assert(256 <= estimate && estimate < 512);
    return estimate;
}
