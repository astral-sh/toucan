#include "abi.h"
#include <stddef.h>
extern int puts(const char *);

_Static_assert(sizeof(void *) == 4 && sizeof(long) == 4, "i686 data model");
_Static_assert(sizeof(long long) == 8 && _Alignof(long long) == 4, "i686 long long");
_Static_assert(sizeof(double) == 8 && _Alignof(double) == 4, "i686 double");
_Static_assert(sizeof(long double) == 12 && _Alignof(long double) == 4, "i686 x87 long double");
_Static_assert(sizeof(struct I686Pair) == 8 && _Alignof(struct I686Pair) == 4, "pair");
_Static_assert(sizeof(struct I686Record) == 28 && _Alignof(struct I686Record) == 4, "record");
_Static_assert(offsetof(struct I686Record, tag) == 0, "record tag");
_Static_assert(offsetof(struct I686Record, value) == 4, "record long");
_Static_assert(offsetof(struct I686Record, number) == 8, "record double");
_Static_assert(offsetof(struct I686Record, context) == 16, "record pointer");
_Static_assert(offsetof(struct I686Record, count) == 20, "record long long");
_Static_assert(sizeof(struct I686Packed) == 5 && _Alignof(struct I686Packed) == 1, "packed");
_Static_assert(offsetof(struct I686Packed, value) == 1, "packed long");
_Static_assert(sizeof(struct I686Bits) == 4 && _Alignof(struct I686Bits) == 4, "bits");
_Static_assert(offsetof(struct I686Bits, tail) == 3, "bits tail");
_Static_assert(I686_MAGIC == 0x7351a20u && I686_MASK == 0x1122334455667788ULL, "constants");

int main(void) {
    puts("i686 C layout: passed");
    return 0;
}
