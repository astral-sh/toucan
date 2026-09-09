void overflow_stores(short a, unsigned long long b, int *result, unsigned long *size) {
    __builtin_add_overflow(a, b, result);
    __builtin_sub_overflow(a, b, result);
    __builtin_mul_overflow(a, b, result);
    __builtin_sadd_overflow(a, b, result);
    __builtin_umull_overflow(a, b, size);
}
