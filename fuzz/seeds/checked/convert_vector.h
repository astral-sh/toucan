typedef int I __attribute__((vector_size(16)));
typedef float F __attribute__((vector_size(16)));
typedef short S __attribute__((vector_size(8)));
I input(void);
void convert(volatile I *p) {
    F f = __builtin_convertvector((input(), *p), const F);
    S s = __builtin_convertvector(__builtin_convertvector(f, I), S);
    _Static_assert(sizeof(__builtin_convertvector(input(), F)) == 16, "size");
    _Static_assert(!__builtin_constant_p(__builtin_convertvector(*p, F)), "query");
    (void)__builtin_choose_expr(1, 0, __builtin_convertvector(input(), F));
    (void)s;
}
