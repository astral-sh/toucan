typedef _Float32 F; typedef _Float64 D; typedef _Float32x X;
struct Pair { F a; D b; };
F add(F a, F b) { return a+b; }
void variadic(int, ...); void values(F a, X b) { variadic(0, a, b); }
#define VALUE (0.1f32)
