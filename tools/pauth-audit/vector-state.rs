//! Full sixteen-nibble QARMA SSSE3 C generator; no runtime Rust/C bridge.
fn table(name: &str, values: &[u8]) {
    print!("static const uint8_t {name}[{}] = {{", values.len());
    for (i, v) in values.iter().enumerate() { if i != 0 { print!(", "); } print!("0x{v:x}"); }
    println!("}};");
}
pub fn emit() {
    let sub = [0xb,6,8,15,12,0,9,14,3,7,4,5,13,2,1,10];
    let mut inv = [0;16];for (i,&v) in sub.iter().enumerate(){inv[v as usize]=i as u8;}
    table("vector_sbox",&sub);table("vector_inv_sbox",&inv);
    let rc = [0_u64,0x13198A2E03707344,0xA4093822299F31D0,0x082EFA98EC4E6C89,0x452821E638D01377];
    for (i,&value) in rc.iter().enumerate(){let mut cells=[0;16];for (n,c)in cells.iter_mut().enumerate(){*c=((value>>(n*4))&15)as u8;}table(&format!("vector_rc{i}"),&cells);}
    print!("{}", r#"
#define V_INLINE static inline __attribute__((target("ssse3"), always_inline))
V_INLINE __m128i vector_unpack(uint64_t value)
{
    __m128i bytes = _mm_cvtsi64_si128(value);
    __m128i mask = _mm_set1_epi8(15);
    return _mm_unpacklo_epi8(_mm_and_si128(bytes, mask),
                           _mm_and_si128(_mm_srli_epi64(bytes, 4), mask));
}
V_INLINE uint64_t vector_pack(__m128i cells)
{
    __m128i bytes = _mm_maddubs_epi16(cells, _mm_set1_epi16(0x1001));
    return _mm_cvtsi128_si64(_mm_packus_epi16(bytes, _mm_setzero_si128()));
}
V_INLINE __m128i vector_shuffle(__m128i v)
{
    return _mm_shuffle_epi8(v, _mm_setr_epi8(13,6,11,0,7,12,1,10,8,3,14,5,2,9,4,15));
}
V_INLINE __m128i vector_inv_shuffle(__m128i v)
{
    return _mm_shuffle_epi8(v, _mm_setr_epi8(3,6,12,9,14,11,1,4,8,13,7,2,5,0,10,15));
}
V_INLINE __m128i vector_mult(__m128i v)
{
    __m128i r1 = _mm_or_si128(_mm_and_si128(_mm_slli_epi16(v, 1), _mm_set1_epi8(14)),
                            _mm_and_si128(_mm_srli_epi16(v, 3), _mm_set1_epi8(1)));
    __m128i r2 = _mm_or_si128(_mm_and_si128(_mm_slli_epi16(v, 2), _mm_set1_epi8(12)),
                            _mm_and_si128(_mm_srli_epi16(v, 2), _mm_set1_epi8(3)));
    return _mm_xor_si128(_mm_xor_si128(_mm_shuffle_epi32(r1, _MM_SHUFFLE(2,1,0,3)),
                                     _mm_shuffle_epi32(r1, _MM_SHUFFLE(0,3,2,1))),
                                     _mm_shuffle_epi32(r2, _MM_SHUFFLE(1,0,3,2)));
}
V_INLINE __m128i vector_tweak(__m128i v)
{
    v = _mm_shuffle_epi8(v, _mm_setr_epi8(4,5,6,7,11,2,3,8,12,13,14,15,0,1,10,9));
    __m128i right = _mm_and_si128(_mm_srli_epi16(v, 1), _mm_set1_epi8(7));
    __m128i rotated = _mm_or_si128(right,
        _mm_slli_epi16(_mm_and_si128(_mm_xor_si128(v,right),_mm_set1_epi8(1)),3));
    __m128i select = _mm_setr_epi8(0,0,15,0,15,0,0,15,0,0,0,15,15,0,15,15);
    return _mm_xor_si128(v, _mm_and_si128(_mm_xor_si128(v,rotated),select));
}
V_INLINE __m128i vector_inv_tweak(__m128i v)
{
    v = _mm_shuffle_epi8(v, _mm_setr_epi8(12,13,5,6,0,1,2,3,7,15,14,4,8,9,10,11));
    __m128i rotated = _mm_or_si128(_mm_and_si128(_mm_slli_epi16(v,1),_mm_set1_epi8(15)),
        _mm_and_si128(_mm_xor_si128(v,_mm_srli_epi16(v,3)),_mm_set1_epi8(1)));
    __m128i select = _mm_setr_epi8(15,0,0,0,0,0,15,0,15,15,15,15,0,0,0,15);
    return _mm_xor_si128(v, _mm_and_si128(_mm_xor_si128(v,rotated),select));
}
V_INLINE __m128i vector_load(const uint8_t *v) { return _mm_loadu_si128((const __m128i*)v); }
__attribute__((target("ssse3"), noinline))
static uint64_t vector_state_pauth_computepac(uint64_t data, uint64_t modifier, ARMPACKey key)
{
    const uint8_t *rc[] = {vector_rc0,vector_rc1,vector_rc2,vector_rc3,vector_rc4};
    const __m128i sbox = vector_load(vector_sbox), inv_sbox = vector_load(vector_inv_sbox);
    const __m128i key0 = vector_unpack(key.hi), key1 = vector_unpack(key.lo);
    const __m128i modk0 = vector_unpack((key.hi << 63) | ((key.hi >> 1) ^ (key.hi >> 63)));
    const __m128i alpha = vector_unpack(UINT64_C(0xC0AC29B7C97C50DD));
    __m128i runningmod = vector_unpack(modifier);
    __m128i workingval = _mm_xor_si128(vector_unpack(data),key0);
    for (int i=0;i<=4;i++) {
        workingval = _mm_xor_si128(workingval,_mm_xor_si128(key1,runningmod));
        workingval = _mm_xor_si128(workingval,vector_load(rc[i]));
        if (i>0) workingval=vector_mult(vector_shuffle(workingval));
        workingval = _mm_shuffle_epi8(sbox,workingval);
        runningmod = vector_tweak(runningmod);
    }
    workingval = _mm_xor_si128(workingval,_mm_xor_si128(modk0,runningmod));
    workingval = vector_mult(vector_shuffle(workingval));
    workingval = _mm_shuffle_epi8(sbox,workingval);
    workingval = vector_mult(vector_shuffle(workingval));
    workingval = _mm_xor_si128(workingval,key1);
    workingval = vector_inv_shuffle(workingval);
    workingval = _mm_shuffle_epi8(inv_sbox,workingval);
    workingval = vector_inv_shuffle(vector_mult(workingval));
    workingval = _mm_xor_si128(workingval,_mm_xor_si128(key0,runningmod));
    for (int i=0;i<=4;i++) {
        workingval = _mm_shuffle_epi8(inv_sbox,workingval);
        if(i<4) workingval=vector_inv_shuffle(vector_mult(workingval));
        runningmod=vector_inv_tweak(runningmod);
        workingval=_mm_xor_si128(workingval,vector_load(rc[4-i]));
        workingval=_mm_xor_si128(workingval,_mm_xor_si128(key1,runningmod));
        workingval=_mm_xor_si128(workingval,alpha);
    }
    return vector_pack(_mm_xor_si128(workingval,modk0));
}
#undef V_INLINE
"#);
}
