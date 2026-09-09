typedef double V __attribute__((vector_size(16)));
_Static_assert(!__builtin_constant_p(__builtin_ia32_undef128()), "not a constant");
V select(V a) { return __builtin_shufflevector(a, __builtin_ia32_undef128(), 1, 0); }
__attribute__((target("no-mmx"))) void discard(void) { (void)__builtin_ia32_undef128(); }
