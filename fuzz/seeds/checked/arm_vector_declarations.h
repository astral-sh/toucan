/* AArch64 retained-code seed. */
typedef __Float32x4_t N;
typedef float G __attribute__((vector_size(16)));
typedef __SVFloat32_t V;
typedef __SVBool_t P;
V sve(V,P);
V f(void);
N __attribute__((aarch64_vector_pcs)) neon(N);
_Static_assert(_Generic((N){0}+(G){0},N:1,G:0),"native identity");
int query(int n){return _Generic(f(),V:1,default:0)+sizeof(int(*)[(f(),n)]);}
/*aaa*/
