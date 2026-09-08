volatile double _Complex z;
volatile double r;
_Atomic(double _Complex) atomic_z;
double next(void);
void projections(void) {
    __real__ z = 2;
    (void)__imag__ z;
    (void)&__real__ z;
    (void)__imag__ r;
    (void)__imag__ next();
    atomic_z++;
    (void)~z;
    (void)__builtin_creal(z);
    (void)__builtin_object_size(&__real__ z, 1);
}
