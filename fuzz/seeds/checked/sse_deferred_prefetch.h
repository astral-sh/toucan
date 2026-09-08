typedef long long V __attribute__((vector_size(16)));
void f(void*p,int locality,int kind){__builtin_ia32_prefetch(p,0,locality,kind);}
V g(V value,int count){return __builtin_ia32_pslldqi128(value,count);}
/*  */
