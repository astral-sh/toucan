typedef int V __attribute__((vector_size(16)));
typedef unsigned int M __attribute__((vector_size(16)));
V f(V a,V b,M mask){return __builtin_shuffle(a,b,mask);}
V g(V a){return __builtin_shuffle(a,(V){4,-1,8,-9});}
/*   */
