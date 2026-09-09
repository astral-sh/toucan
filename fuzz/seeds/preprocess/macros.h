#define JOIN(a, b) a ## b
#define VALUE 7
#define TWICE(x) ((x) + (x))
#if defined(VALUE) && TWICE(VALUE) == 14
enum { JOIN(API, _VALUE) = VALUE };
#endif
