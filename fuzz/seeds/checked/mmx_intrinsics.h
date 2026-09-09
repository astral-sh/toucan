typedef int V __attribute__((vector_size(8)));
typedef short S __attribute__((vector_size(8)));
V f(V value,int n){
 V shifted=__builtin_ia32_pslldi(value,n);
 V packed=__builtin_ia32_pmaddwd((S){1,2,3,4},(S){4,3,2,1});
 __builtin_ia32_emms();
 return shifted+packed;
}
int g(V value){return __builtin_ia32_vec_ext_v2si(value,__builtin_ctz(2.0));}
/*   */
