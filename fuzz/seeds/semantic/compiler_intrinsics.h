double sum(int n, ...) {
    __builtin_va_list a, copy;
    __builtin_va_start(a, n);
    __builtin_va_copy(copy, a);
    double result = 0;
    for (int i = 0; i < __builtin_expect(n, 1); ++i)
        result += __builtin_va_arg(copy, double);
    __builtin_va_end(copy);
    __builtin_va_end(a);
    return result;
}
