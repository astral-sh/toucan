void prefetch_values(int *p, int *volatile *q) {
    __builtin_prefetch(p++);
    __builtin_prefetch(*q, 1LL, 3LL);
}
int prefetch_query(int n, int (*p)[n]) {
    return __builtin_constant_p((__builtin_prefetch(p++), 1));
}
