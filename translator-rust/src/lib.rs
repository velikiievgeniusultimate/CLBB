// SPDX-License-Identifier: LGPL-2.0-or-later
// Copyright (c) 2019 Linaro, Ltd. (original QEMU implementation).
// QARMA/PAuth arithmetic port of QEMU target/arm/pauth_helper.c.
// Original QEMU authorship/license remains applicable; see README.md.
#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(target_arch = "x86_64")]
mod x86 {
    use core::arch::x86_64::*;

    // Only fixed-size value conversions: no raw pointers or external memory.
    #[inline]
    fn lanes(bytes: [u8; 16]) -> __m128i {
        // SAFETY: both types are 16 bytes; every bit pattern is valid.
        unsafe { core::mem::transmute(bytes) }
    }

    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn unpack(x: u64) -> __m128i {
        let b = _mm_cvtsi64_si128(x as i64);
        let mask = _mm_set1_epi8(15);
        _mm_unpacklo_epi8(_mm_and_si128(b, mask), _mm_and_si128(_mm_srli_epi64::<4>(b), mask))
    }
    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn pack(x: __m128i) -> u64 {
        let b = _mm_maddubs_epi16(x, _mm_set1_epi16(0x1001));
        _mm_cvtsi128_si64(_mm_packus_epi16(b, _mm_setzero_si128())) as u64
    }
    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn shuffle(x: __m128i) -> __m128i {
        _mm_shuffle_epi8(x, lanes([13,6,11,0,7,12,1,10,8,3,14,5,2,9,4,15]))
    }
    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn inverse_shuffle(x: __m128i) -> __m128i {
        _mm_shuffle_epi8(x, lanes([3,6,12,9,14,11,1,4,8,13,7,2,5,0,10,15]))
    }
    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn mult(x: __m128i) -> __m128i {
        let r1 = _mm_or_si128(_mm_and_si128(_mm_slli_epi16::<1>(x), _mm_set1_epi8(14)),
                              _mm_and_si128(_mm_srli_epi16::<3>(x), _mm_set1_epi8(1)));
        let r2 = _mm_or_si128(_mm_and_si128(_mm_slli_epi16::<2>(x), _mm_set1_epi8(12)),
                              _mm_and_si128(_mm_srli_epi16::<2>(x), _mm_set1_epi8(3)));
        _mm_xor_si128(_mm_xor_si128(_mm_shuffle_epi32::<0x93>(r1), _mm_shuffle_epi32::<0x39>(r1)),
                      _mm_shuffle_epi32::<0x4e>(r2))
    }
    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn tweak(x: __m128i) -> __m128i {
        let x = _mm_shuffle_epi8(x, lanes([4,5,6,7,11,2,3,8,12,13,14,15,0,1,10,9]));
        let right = _mm_and_si128(_mm_srli_epi16::<1>(x), _mm_set1_epi8(7));
        let rot = _mm_or_si128(right, _mm_slli_epi16::<3>(_mm_and_si128(_mm_xor_si128(x,right),_mm_set1_epi8(1))));
        _mm_xor_si128(x, _mm_and_si128(_mm_xor_si128(x,rot),lanes([0,0,15,0,15,0,0,15,0,0,0,15,15,0,15,15])))
    }
    #[inline]
    #[target_feature(enable = "ssse3")]
    unsafe fn inverse_tweak(x: __m128i) -> __m128i {
        let x = _mm_shuffle_epi8(x, lanes([12,13,5,6,0,1,2,3,7,15,14,4,8,9,10,11]));
        let rot = _mm_or_si128(_mm_and_si128(_mm_slli_epi16::<1>(x),_mm_set1_epi8(15)),
                              _mm_and_si128(_mm_xor_si128(x,_mm_srli_epi16::<3>(x)),_mm_set1_epi8(1)));
        _mm_xor_si128(x, _mm_and_si128(_mm_xor_si128(x,rot),lanes([15,0,0,0,0,0,15,0,15,15,15,15,0,0,0,15])))
    }

    /// Complete PAC arithmetic. Caller must establish host SSSE3 support.
    /// No pointers, allocation, guest state, syscalls or mutable globals.
    #[no_mangle]
    #[target_feature(enable = "ssse3")]
    pub unsafe extern "C" fn clbb_rust_pac_ssse3(data: u64, modifier: u64, key_hi: u64, key_lo: u64) -> u64 {
        // SAFETY: the caller guarantees SSSE3; all helpers operate on values.
        unsafe {
            let rc = [0,0x13198A2E03707344,0xA4093822299F31D0,0x082EFA98EC4E6C89,0x452821E638D01377];
            let sub = lanes([11,6,8,15,12,0,9,14,3,7,4,5,13,2,1,10]);
            let inv = lanes([5,14,13,8,10,11,1,9,2,6,15,0,4,12,7,3]);
            let k0 = unpack(key_hi);
            let k1 = unpack(key_lo);
            let mk = unpack((key_hi << 63) | ((key_hi >> 1) ^ (key_hi >> 63)));
            let alpha = unpack(0xC0AC29B7C97C50DD);
            let mut m = unpack(modifier);
            let mut w = _mm_xor_si128(unpack(data),k0);
            for (i,r) in rc.into_iter().enumerate() {
                w = _mm_xor_si128(w,_mm_xor_si128(k1,m));
                w = _mm_xor_si128(w,unpack(r));
                if i > 0 { w = mult(shuffle(w)); }
                w = _mm_shuffle_epi8(sub,w);
                m = tweak(m);
            }
            w = _mm_xor_si128(w,_mm_xor_si128(mk,m));
            w = _mm_shuffle_epi8(sub,mult(shuffle(w)));
            w = mult(shuffle(w));
            w = inverse_shuffle(_mm_xor_si128(w,k1));
            w = _mm_shuffle_epi8(inv,w);
            w = inverse_shuffle(mult(w));
            w = _mm_xor_si128(w,_mm_xor_si128(k0,m));
            for (i,r) in rc.into_iter().rev().enumerate() {
                w = _mm_shuffle_epi8(inv,w);
                if i < 4 { w = inverse_shuffle(mult(w)); }
                m = inverse_tweak(m);
                w = _mm_xor_si128(w,unpack(r));
                w = _mm_xor_si128(w,_mm_xor_si128(k1,m));
                w = _mm_xor_si128(w,alpha);
            }
            pack(_mm_xor_si128(w,mk))
        }
    }
}
