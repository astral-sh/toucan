typedef void V __attribute__((aligned(32)));
typedef void F(void) __attribute__((aligned(32)));
struct P { V *data; F *callback; };
void action(void);
enum { VOID_SIZE = sizeof(V), FUNCTION_SIZE = sizeof(F), VOID_ALIGN = __alignof__(V), FUNCTION_ALIGN = _Alignof(F) };
void queries(volatile V *p) {
    sizeof(*p);
    sizeof(action());
    sizeof(__builtin_prefetch((void*)0));
    _Alignof(*p);
    (void)*p;
}
