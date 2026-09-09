#define JOIN(a,b) a ## b
#define APPLY(f,...) f(__VA_ARGS__)
#define PICK(_0,_1,_2,...) _2
#define NAME(n) JOIN(value_,n)
#if defined(__GNUC__) && !defined(MISSING)
int NAME(APPLY(PICK,0,1,2));
#endif
#define TWICE(x) ((x)+(x))
int value = TWICE(TWICE(3));
