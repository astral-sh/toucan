enum { CAST = (int)0x1.fp2, ROUNDED = (int)16777217.0f };
float decimal = 0.10000000000000000000000000000000000000000000000000001;
long double subnormal = 0x1p-16445L;
int comparison = 0x1p63L + 1.0L != 0x1p63L;
int conditional = 0.0 ? (int)(1.0 / 0.0) : (int)-3.75;
unsigned fraction = (unsigned)-0.5;
