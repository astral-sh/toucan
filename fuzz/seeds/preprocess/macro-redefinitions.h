#define VALUE 1
#define VALUE 2
#define CALL(x) ((x) + VALUE)
#define CALL(y) ((y) + VALUE)
#undef VALUE
#define VALUE 3
#if 0
#define VALUE 4
#endif
CALL(7)
