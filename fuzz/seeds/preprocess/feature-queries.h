#define B __builtin_bswap32
#define ATTR aligned
#define QUERY(x) __has_builtin(x)
#if defined(__has_builtin) && __has_attribute(ATTR)
int query = QUERY(B);
#endif
int raw_query = __has_builtin(B);
#define FINAL __has_builtin(__builtin_bswap32)
#undef __has_builtin
#define __has_builtin(x) 7
int replaced = FINAL;
