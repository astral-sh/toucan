struct Precision { signed small:3; unsigned narrow:1; };
int overflow_predicates(struct Precision *p, volatile int *observed, int n) {
    __builtin_add_overflow_p(3, 1, p->small);
    __builtin_mul_overflow_p(3, 1, p->narrow);
    __builtin_sub_overflow_p(1, 2, *observed);
    __builtin_add_overflow_p(1, 2, n++);
    __builtin_mul_overflow_p(1, 2, sizeof(int[n++]));
    return __builtin_constant_p(__builtin_add_overflow_p(1, 2, ({ struct Local { int x; }; int value=0; value; })));
}
