typedef _Float16 H;
typedef __bf16 B;
H half = (_Float16)1.00048828125;
B brain = (__bf16)-0.0;
typedef H HV __attribute__((vector_size(16)));
struct Pair { H h; B b; };
void variadic(int, ...);
B combine(H h, B b) { variadic(1, h, b, 1.0f); b += h; return b; }
HV vector_add(HV x, HV y) { return x + y; }
_Static_assert(sizeof(struct Pair) == 4, "distinct narrow fields");
