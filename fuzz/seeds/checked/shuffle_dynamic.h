typedef int V __attribute__((vector_size(16)));
typedef signed char M __attribute__((vector_size(4)));
V pick(V a,M m){return __builtin_shufflevector(a,m);}
