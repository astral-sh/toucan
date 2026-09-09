typedef int V __attribute__((vector_size(16)));
typedef float F __attribute__((vector_size(16)));
int bound(void);
void hints(volatile int *volatile p, volatile double value, V *bits, F lanes) {
    __builtin_nontemporal_store(value, p);
    __builtin_nontemporal_store(lanes, bits);
    (void)__builtin_nontemporal_load(p);
    _Static_assert(!__builtin_constant_p(__builtin_nontemporal_load(p)), "query");
}
void extents(int n, int (*p)[n]) {
    int (*q)[n] = __builtin_nontemporal_load(&p);
    __builtin_nontemporal_store(q, &p);
    (void)__builtin_object_size((int (*)[bound()])__builtin_nontemporal_load(&p), 0);
}
