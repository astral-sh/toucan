typedef float F __attribute__((vector_size(16)));
typedef int V __attribute__((vector_size(16)));
F f(F a,F b){return __builtin_ia32_shufps(__builtin_ia32_sqrtps(a),b,27);}
V g(F a){return __builtin_ia32_cvttps2dq(a);}
int query(F a){return __builtin_constant_p(__builtin_ia32_movmskps(a));}
/**/
