int bound(void);
void hints(const volatile _Complex double *p, _Complex double *destination, _Complex float value) {
    __builtin_nontemporal_store(1.25, destination);
    __builtin_nontemporal_store(value, destination);
    __builtin_nontemporal_store(__builtin_nontemporal_load(p), destination);
    _Static_assert(!__builtin_constant_p((double)__builtin_nontemporal_load(p)), "query");
    (void)__builtin_object_size((int (*)[bound()])(unsigned long long)(double)__builtin_nontemporal_load(p), 0);
}
