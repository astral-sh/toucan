struct Fields { unsigned value:3; };
unsigned long long convert_bytes(unsigned char *destination, const unsigned char *source,
                                 unsigned long count, struct Fields fields) {
    __builtin_memset(destination, fields.value, count);
    __builtin_memcpy(destination, source, count);
    __builtin_memmove(destination + 1, destination, count - 1);
    int order = __builtin_memcmp(destination, source, count);
    unsigned short small = __builtin_bswap16((unsigned short)0x12345);
    unsigned wide = __builtin_bswap32(1.75 + order);
    return __builtin_bswap64((unsigned long long)wide << 16 | small);
}
int builtin_shadow(int (*__builtin_memcpy)(int), int (*__builtin_bswap16)(int)) {
    return __builtin_memcpy(__builtin_bswap16(7));
}
_Static_assert(sizeof(__builtin_bswap16(1)) == 2, "swap width");
_Static_assert(sizeof(__builtin_bswap32(1)) == 4, "swap width");
_Static_assert(sizeof(__builtin_bswap64(1)) == 8, "swap width");
