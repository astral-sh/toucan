#include <stdint.h>
#include <stddef.h>
#include <stdarg.h>
#include <stdatomic.h>
#include <wchar.h>

#define MUSL_MASK UINT64_C(0x8123456789abcdef)
#define MUSL_CHAR_UNSIGNED ((char)-1 > 0)
struct MuslPair { float x, y; };
struct MuslTriplet { unsigned char x, y, z; };
struct MuslMixed { char tag; long value; double number; void *pointer; };
struct MuslPacked { char tag; long value; } __attribute__((packed));
struct MuslBits { signed value:5; unsigned flag:3; unsigned rest:9; char tail; };
struct MuslAtomic { _Atomic unsigned long value; };
union MuslUnion { uint64_t integer; double floating; };
typedef struct MuslPair (*MuslCallback)(struct MuslPair, long, void *);

struct MuslPair musl_pair(struct MuslPair value);
struct MuslTriplet musl_triplet(struct MuslTriplet value);
struct MuslMixed musl_mixed(struct MuslMixed value);
struct MuslPacked musl_packed(struct MuslPacked value);
union MuslUnion musl_union(union MuslUnion value);
struct MuslPair musl_callback(MuslCallback callback, struct MuslPair value, void *context);
long musl_stack(long a, long b, long c, long d, long e, long f, long g, long h,
               double i, double j, double k, double l, double m, double n, double o, double p, double q);
long musl_variadic(int marker, ...);
void musl_bits_write(struct MuslBits *value);
long musl_bits_read(const struct MuslBits *value);
void musl_atomic_store(struct MuslAtomic *value, unsigned long input);
unsigned long musl_atomic_load(const struct MuslAtomic *value);
void musl_dimensions(size_t *values);
