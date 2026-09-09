_Static_assert(__builtin_clzl(1) == sizeof(unsigned long) * 8 - 1, "target long width");
_Static_assert(__builtin_clzll(1) == 63, "long long width");
_Static_assert(__builtin_ctz(0x100000008ULL) == 3, "parameter conversion");
int bit_counts(unsigned long long value) {
    int result = __builtin_clz(value) + __builtin_ctz(value);
    result += __builtin_clzl(value) + __builtin_ctzl(value);
    result += __builtin_clzll(value) + __builtin_ctzll(value);
    return result;
}
int undefined_runtime(void) { return __builtin_clz(0); }
/*a*/
