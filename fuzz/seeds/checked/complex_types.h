typedef double _Complex Complex;
struct Pair { float _Complex small; Complex value; };
_Atomic(Complex) shared;
Complex multiply(Complex value, double scale) {
    shared *= value;
    return value * scale + __builtin_complex(scale, -0.0);
}
int condition(Complex value) {
    return value ? value == 2.0i : !value;
}
int constant_query(void) {
    return __builtin_constant_p(__builtin_complex(1.0, 2.0));
}
void variadic(int, ...);
void promote(float _Complex value) {
    variadic(0, value);
}
