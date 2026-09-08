#include <stdarg.h>
double sum(int n, ...) { va_list args, copy; va_start(args, n); va_copy(copy, args); double total=0; for (int i=0; i<n; ++i) total += va_arg(copy, double); va_end(copy); va_end(args); return total; }
