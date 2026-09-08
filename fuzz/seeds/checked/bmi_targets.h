__attribute__((target("lzcnt,bmi,bmi2"), always_inline))
inline unsigned bit_helper(unsigned x) { return x ^ 3u; }
__attribute__((target("no-bmi2,bmi2,lzcnt,bmi")))
unsigned bit_caller(unsigned x) { return bit_helper(x); }
unsigned dormant_bit_caller(unsigned x) {
    if (0) return bit_helper(x);
    return sizeof(bit_helper(x));
}
