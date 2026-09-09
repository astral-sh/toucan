#include "zstd.h"

_Static_assert(sizeof(ZSTD_inBuffer) == 12, "ZSTD_inBuffer size");
_Static_assert(_Alignof(ZSTD_inBuffer) == 4, "ZSTD_inBuffer alignment");
_Static_assert(__builtin_offsetof(ZSTD_inBuffer, src) == 0, "ZSTD_inBuffer src");
_Static_assert(__builtin_offsetof(ZSTD_inBuffer, size) == 4, "ZSTD_inBuffer size field");
_Static_assert(__builtin_offsetof(ZSTD_inBuffer, pos) == 8, "ZSTD_inBuffer pos field");

_Static_assert(sizeof(ZSTD_outBuffer) == 12, "ZSTD_outBuffer size");
_Static_assert(_Alignof(ZSTD_outBuffer) == 4, "ZSTD_outBuffer alignment");
_Static_assert(__builtin_offsetof(ZSTD_outBuffer, dst) == 0, "ZSTD_outBuffer dst");
_Static_assert(__builtin_offsetof(ZSTD_outBuffer, size) == 4, "ZSTD_outBuffer size field");
_Static_assert(__builtin_offsetof(ZSTD_outBuffer, pos) == 8, "ZSTD_outBuffer pos field");
