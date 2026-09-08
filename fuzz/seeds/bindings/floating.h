#define FLOAT_CAST ((int)0x1.fp2)
#define FLOAT_ROUNDED ((unsigned long long)9007199254740993.0)
#define FLOAT_OVERFLOW ((int)2147483648.0)
#define FLOAT_NEGATIVE_ZERO (-0.0f)
#define FLOAT_SUBNORMAL (0x1p-149f)
#define FLOAT_ARITHMETIC (1.0 / 3.0)
#define FLOAT_WIDE (0x1p63L + 1.0L)
#define FLOAT_NONFINITE (__builtin_nanf(""))
extern float sample(float value, double scale);
