int query(int n, int a, int b, int (*p)[n]) {
    static int constant = __builtin_constant_p(sizeof(int[n++]));
    return constant + __builtin_constant_p(n ? sizeof *(p++) : sizeof(typeof(*(int (*)[a++])0)))
        + __builtin_constant_p(__builtin_nanf("1") + sizeof(int[b++]));
}
