//! Finite-domain audit prototype; no floating-point arithmetic is used.
#![deny(unsafe_op_in_unsafe_fn)]
pub const FIRST: u32 = 128;
pub const LAST: u32 = 511;
pub const COUNT: usize = (LAST - FIRST + 1) as usize;

/// Integer definition of the finite ARM estimate mapping. u64 intermediates
/// make generation independent of host signed-overflow rules. The exhaustive
/// C-oracle comparison separately checks the original production int version.
pub const fn estimate(mut a: u32) -> u16 {
    assert!(a >= FIRST && a <= LAST);
    if a < 256 {
        a = a * 2 + 1;
    } else {
        a = ((a >> 1) << 1) + 1;
        a *= 2;
    }
    let mut b = 512u64;
    while (a as u64) * (b + 1) * (b + 1) < (1u64 << 28) {
        b += 1;
    }
    let out = ((b + 1) / 2) as u16;
    assert!(out >= 256 && out < 512);
    out
}
const fn build_table() -> [u8; COUNT] {
    let mut out = [0; COUNT];
    let mut i = 0;
    while i < COUNT {
        out[i] = (estimate(FIRST + i as u32) - 256) as u8;
        i += 1;
    }
    out
}
pub const TABLE: [u8; COUNT] = build_table();

/// Native host ABI prototype, not a guest/QEMU thunk. Production C integration
/// should prefer the generated inline C header to avoid even this call.
///
/// # Safety
/// The caller must supply 128 <= a < 512, exactly the production helper's
/// asserted precondition. Invalid inputs are outside the audited domain.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clbb_rust_rsqrt_lookup(a: i32) -> i32 {
    debug_assert!((128..512).contains(&a));
    256 + i32::from(unsafe { *TABLE.get_unchecked((a - 128) as usize) })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_384_inputs_and_result_range() {
        assert_eq!(COUNT, 384);
        for a in FIRST..=LAST {
            let expected = estimate(a);
            assert!((256..512).contains(&expected));
            assert_eq!(256 + u16::from(TABLE[(a - FIRST) as usize]), expected);
            assert_eq!(
                unsafe { clbb_rust_rsqrt_lookup(a as i32) },
                i32::from(expected)
            );
        }
    }
    #[test]
    fn every_caller_prefix_is_in_domain() {
        // Floating wrappers explicitly prepend binary 01 (odd exponent)
        // or binary 1 (even exponent), before their extracted fraction bits.
        for frac in 0..128 {
            assert!((FIRST..=LAST).contains(&(128 | frac)));
        }
        for frac in 0..256 {
            assert!((FIRST..=LAST).contains(&(256 | frac)));
        }
        // Unsigned helper tests the top two input bits before extracting 9.
        for prefix in 0u32..512 {
            let input = prefix << 23;
            if input & 0xc0000000 != 0 {
                assert!((FIRST..=LAST).contains(&prefix));
            }
        }
    }
    #[test]
    #[should_panic]
    fn generation_rejects_invalid_input() {
        estimate(127);
    }
}
