typedef int V __attribute__((vector_size(16)));
V a; V b;
V pick(void){return __builtin_shufflevector(a,b,7,-1,2,4);}
int query(void){return __builtin_constant_p(__builtin_shufflevector(a,b,0,1));}
