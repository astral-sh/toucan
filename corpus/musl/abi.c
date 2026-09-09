#include "abi.h"
#include <assert.h>
struct MuslPair musl_pair(struct MuslPair v) { v.x += 2.0f; v.y -= 3.0f; return v; }
struct MuslTriplet musl_triplet(struct MuslTriplet v) { v.x += 1; v.y += 2; v.z += 3; return v; }
struct MuslMixed musl_mixed(struct MuslMixed v) { v.tag = 7; v.value += 17; v.number *= 2; return v; }
struct MuslPacked musl_packed(struct MuslPacked v) { v.tag = 9; v.value += 19; return v; }
union MuslUnion musl_union(union MuslUnion v) { v.integer ^= MUSL_MASK; return v; }
struct MuslPair musl_callback(MuslCallback cb, struct MuslPair v, void *ctx) { return cb(cb(v, 5, ctx), 11, ctx); }
long musl_stack(long a, long b, long c, long d, long e, long f, long g, long h,
               double i, double j, double k, double l, double m, double n, double o, double p, double q) {
    return a + 2*b + 3*c + 4*d + 5*e + 6*f + 7*g + 8*h + (long)(i+2*j+3*k+4*l+5*m+6*n+7*o+8*p+9*q);
}
static long musl_variadic_copy(va_list ap) {
    va_list copy; va_copy(copy, ap);
    int a = va_arg(copy, int); double b = va_arg(copy, double);
    long long c = va_arg(copy, long long); const long *d = va_arg(copy, const long *);
    va_end(copy); return a + (long)b + (long)c + *d;
}
long musl_variadic(int marker, ...) {
    va_list ap; va_start(ap, marker); long result=musl_variadic_copy(ap); va_end(ap); return marker+result;
}
void musl_bits_write(struct MuslBits *v) { v->value = -9; v->flag = 6; v->rest = 381; v->tail = 12; }
long musl_bits_read(const struct MuslBits *v) { return v->value + 100*v->flag + 1000*v->rest + 1000000*v->tail; }
void musl_atomic_store(struct MuslAtomic *v, unsigned long input) { atomic_store(&v->value, input); }
unsigned long musl_atomic_load(const struct MuslAtomic *v) { return atomic_load(&v->value); }
#define SA(T) sizeof(T), _Alignof(T)
void musl_dimensions(size_t *values) {
    const size_t expected[] = {
        SA(char), SA(short), SA(int), SA(long), SA(long long), SA(void*), SA(wchar_t), SA(wint_t),
        SA(float), SA(double), SA(long double), SA(va_list),
        SA(struct MuslPair), SA(struct MuslTriplet), SA(struct MuslMixed), SA(struct MuslPacked),
        SA(struct MuslBits), SA(struct MuslAtomic), SA(union MuslUnion),
        offsetof(struct MuslPair,y), offsetof(struct MuslTriplet,z), offsetof(struct MuslMixed,value),
        offsetof(struct MuslMixed,number), offsetof(struct MuslMixed,pointer), offsetof(struct MuslPacked,value),
        offsetof(struct MuslBits,tail), MUSL_CHAR_UNSIGNED,
        _Generic(UINT64_C(1),unsigned long:1,default:0), _Generic((wchar_t)0,unsigned int:1,default:0),
        _Generic((wint_t)0,unsigned int:1,default:0)
    };
    for (size_t i=0;i<sizeof(expected)/sizeof(expected[0]);i++) values[i]=expected[i];
}
