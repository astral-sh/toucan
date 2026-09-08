/* AArch64 retained-code seed. */
typedef __SVFloat32_t V;
typedef __SVFloat64_t D;
V f(void);
D __attribute__((aarch64_sve_pcs)) g(D);
int query(int n){__typeof__(f()) *p; if(0)f();return _Generic((V){f()},V:1,default:0)+_Alignof(int[(f(),n)]);}
/**/
