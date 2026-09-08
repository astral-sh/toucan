int callback(int value);
int derived(int n) {
    typedef int A[n];
    A a;
    __auto_type *p = &a, *q = &a;
    __auto_type (*call)(int argument) = callback, scalar = 1;
    __auto_type x = n++, y = n++;
    int constant = __builtin_constant_p(({ __auto_type v = 1, w = v + 1; v + w; }));
    return call(scalar) + sizeof *p + sizeof *q + x + y + constant;
}
