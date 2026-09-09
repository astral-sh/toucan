enum {
    leading = __builtin_constant_p(__builtin_clz(8)),
    swapped = __builtin_constant_p(__builtin_ctz(__builtin_bswap32(0x12345678))),
    infinity = __builtin_constant_p(__builtin_inf()),
    quiet = __builtin_constant_p(__builtin_nan("0x12")),
    signaling = __builtin_constant_p(__builtin_nansl("0x12"))
};
int query(int value) {
    return __builtin_constant_p(__builtin_ctz(value++));
}
