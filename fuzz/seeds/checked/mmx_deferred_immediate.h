typedef int V __attribute__((vector_size(8)));
int f(V value,int n){return __builtin_ia32_vec_ext_v2si(value,(n++,1));}
/* */
