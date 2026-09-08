typedef __typeof__(1.0Q) Quad;
typedef __typeof__(1.0Qi) Complex;
Quad real;
Complex value;
_Atomic(Complex) shared;
_Static_assert(sizeof(Quad) == 16 && sizeof(Complex) == 32, "storage");
void operations(void) {
  value = real + value;
  shared += real;
  __real__ value = 1.0Q + 0x1p-112Q;
  (void)__imag__ ~value;
  (void)__builtin_types_compatible_p(Quad, long double);
}
