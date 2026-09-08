typedef signed char V __attribute__((vector_size(16)));
int effect(void);
int elementwise(signed char a, unsigned short b, _Atomic int *p, V x, V y) {
    V s = __builtin_elementwise_add_sat(x,y);
    s = __builtin_elementwise_sub_sat(s,x);
    s = __builtin_elementwise_min(s,x);
    s = __builtin_elementwise_max(s,y);
    _Static_assert(!__builtin_constant_p(__builtin_elementwise_add_sat(1,2)), "query");
    (void)__builtin_choose_expr(1,0,__builtin_elementwise_min(effect(),1));
    return __builtin_elementwise_min(a,b)+__builtin_elementwise_max(*p,1)+s[0];
}
