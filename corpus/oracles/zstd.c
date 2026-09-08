/* Include the pinned zstd.h before this file. */
#include <limits.h>
#include <stdint.h>

_Static_assert(ZSTD_CONTENTSIZE_UNKNOWN == UINT64_MAX,
               "unknown content size is the maximum unsigned 64-bit value");
_Static_assert(ZSTD_CONTENTSIZE_ERROR == UINT64_MAX - 1,
               "content-size error is the maximum unsigned 64-bit value minus one");
_Static_assert(sizeof(ZSTD_CONTENTSIZE_UNKNOWN) * CHAR_BIT == 64,
               "unknown content size is 64 bits");
_Static_assert(sizeof(ZSTD_CONTENTSIZE_ERROR) * CHAR_BIT == 64,
               "content-size error is 64 bits");
_Static_assert(_Generic(ZSTD_CONTENTSIZE_UNKNOWN,
                       unsigned long long: 1, default: 0),
               "unknown content size has unsigned long long type");
_Static_assert(_Generic(ZSTD_CONTENTSIZE_ERROR,
                       unsigned long long: 1, default: 0),
               "content-size error has unsigned long long type");

/* C11 enumerator expressions have type int, independently of the enum type. */
_Static_assert(_Generic(ZSTD_fast, int: 1, default: 0),
               "ZSTD_fast has type int");
_Static_assert(_Generic(ZSTD_c_compressionLevel, int: 1, default: 0),
               "ZSTD_c_compressionLevel has type int");
_Static_assert(ZSTD_fast == 1 && ZSTD_c_compressionLevel == 100,
               "representative enumerator values match the pinned header");
