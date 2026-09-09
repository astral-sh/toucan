static float positive = __builtin_inff();
static double negative = -__builtin_huge_val();
static long double extended = __builtin_infl();
float infinite_sum(float x) { return x + __builtin_huge_valf(); }
double infinity_cast(void) { return (double)__builtin_huge_vall(); }
int infinite_test(void) { return __builtin_inf() > 1.0; }
