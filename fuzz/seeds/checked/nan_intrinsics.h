static float quiet = __builtin_nanf("0x12345");
static double signaling = -__builtin_nans("1");
static long double extended = __builtin_nanl("077");
double widen(void) { return (double)__builtin_nansf("1"); }
float payload(const char *text) { return __builtin_nansf(text); }
int unordered(void) { return __builtin_nan("1") != __builtin_nan("2"); }
