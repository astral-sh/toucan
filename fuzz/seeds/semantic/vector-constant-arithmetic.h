typedef int I __attribute__((vector_size(16)));
typedef unsigned char B __attribute__((vector_size(16)));
typedef float F __attribute__((vector_size(16)));
B wrapped = (B){255, 0, 128, 2} + (B){1, 255, 128, 254};
I shifted = (I){-1, -8, 16, 64} >> 2L;
I comparison = (F){1, 2, 3, 4} <= (F){0, 2, 4, 1};
F rounded = (F){16777216.0f, 1.5f, -0.0f, 0x1p-149f} + (F){1.0f, 2.25f, -0.0f, 0x1p-149f};
