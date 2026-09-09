#include "abi.h"
extern int puts(const char *);

#if !defined(__ARM_PCS_VFP) || !defined(__ARM_ARCH_7A__)
#error "expected ARMv7-A with hard-float procedure calls"
#endif

_Static_assert(sizeof(void *) == 4 && sizeof(long) == 4, "ARMv7 data model");
_Static_assert(sizeof(long long) == 8 && _Alignof(long long) == 8, "ARMv7 long long");
_Static_assert(sizeof(double) == 8 && _Alignof(double) == 8, "ARMv7 double");
_Static_assert(sizeof(long double) == 8 && _Alignof(long double) == 8, "ARMv7 long double");
_Static_assert(sizeof(struct Armv7Pair) == 8 && _Alignof(struct Armv7Pair) == 4, "pair");
_Static_assert(sizeof(struct Armv7Record) == 32 && _Alignof(struct Armv7Record) == 8, "record");
_Static_assert(__builtin_offsetof(struct Armv7Record, tag) == 0, "record tag");
_Static_assert(__builtin_offsetof(struct Armv7Record, value) == 4, "record long");
_Static_assert(__builtin_offsetof(struct Armv7Record, number) == 8, "record double");
_Static_assert(__builtin_offsetof(struct Armv7Record, context) == 16, "record pointer");
_Static_assert(__builtin_offsetof(struct Armv7Record, count) == 24, "record long long");
_Static_assert(sizeof(struct Armv7Packed) == 5 && _Alignof(struct Armv7Packed) == 1, "packed");
_Static_assert(__builtin_offsetof(struct Armv7Packed, value) == 1, "packed long");
_Static_assert(sizeof(struct Armv7Bits) == 4 && _Alignof(struct Armv7Bits) == 4, "bits");
_Static_assert(__builtin_offsetof(struct Armv7Bits, tail) == 3, "bits tail");
_Static_assert(ARMV7_MAGIC == 0x7351a20u && ARMV7_MASK == 0x1122334455667788ULL, "constants");

int main(void) {
    puts("ARMv7 hard-float C layout: passed");
    return 0;
}
