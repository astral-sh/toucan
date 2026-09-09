typedef int I __attribute__((vector_size(16)));
typedef unsigned U __attribute__((vector_size(16)));
typedef float F __attribute__((vector_size(16)));
typedef short S __attribute__((vector_size(8)));
int calls;
I input(void) { ++calls; return (I){-12, 0, 5, 32767}; }
int main(void) {
    F f = __builtin_convertvector(input(), F);
    if (calls != 1 || f[0] != -12.0f || f[1] != 0.0f || f[2] != 5.0f || f[3] != 32767.0f) return 1;
    I i = __builtin_convertvector((F){-2.75f, -0.0f, 1.9f, 12345.75f}, I);
    if (i[0] != -2 || i[1] != 0 || i[2] != 1 || i[3] != 12345) return 2;
    U u = __builtin_convertvector((I){-1, 0, 1, -2}, U);
    if (u[0] != ~0u || u[1] != 0 || u[2] != 1 || u[3] != ~1u) return 3;
    S s = __builtin_convertvector((I){-32768, -1, 0, 32767}, S);
    i = __builtin_convertvector(s, I);
    if (i[0] != -32768 || i[1] != -1 || i[2] != 0 || i[3] != 32767) return 4;
    volatile I v = {-4, 3, 2, 1};
    f = __builtin_convertvector(v, F);
    if (f[0] != -4 || f[3] != 1) return 5;
    if (sizeof(__builtin_convertvector(input(), F)) != 16 || calls != 1) return 6;
    if (__builtin_constant_p(__builtin_convertvector(input(), F)) || calls != 1) return 7;
    return 0;
}
