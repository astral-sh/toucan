/* Include the pinned git2.h before this file. */
#include <limits.h>
#include <stddef.h>
#include <stdint.h>

_Static_assert(GIT_OBJECT_SIZE_MAX == UINT64_MAX,
               "maximum object size is the maximum unsigned 64-bit value");
_Static_assert(sizeof(GIT_OBJECT_SIZE_MAX) * CHAR_BIT == 64,
               "maximum object size is 64 bits");
_Static_assert(_Generic(GIT_OBJECT_SIZE_MAX,
                       unsigned long: 1, unsigned long long: 1, default: 0),
               "maximum object size has unsigned 64-bit type");

_Static_assert(GIT_REBASE_NO_OPERATION == SIZE_MAX,
               "no rebase operation is the maximum size_t value");
_Static_assert(GIT_REBASE_NO_OPERATION == (size_t)-1,
               "no rebase operation preserves every size_t bit");
_Static_assert(sizeof(GIT_REBASE_NO_OPERATION) == sizeof(size_t),
               "no rebase operation has size_t width");
_Static_assert(sizeof(GIT_REBASE_NO_OPERATION) * CHAR_BIT == 64,
               "the supported corpus targets have 64-bit size_t");
_Static_assert(_Generic(GIT_REBASE_NO_OPERATION, size_t: 1, default: 0),
               "no rebase operation has size_t type");
_Static_assert(_Generic(GIT_REBASE_NO_OPERATION,
                       unsigned int: 1, unsigned long: 1,
                       unsigned long long: 1, default: 0),
               "no rebase operation has an unsigned integer type");

/* The enum's compatible type does not change the type of its enumerators. */
_Static_assert(_Generic(GIT_BRANCH_LOCAL, int: 1, default: 0),
               "GIT_BRANCH_LOCAL has type int");
_Static_assert(_Generic(GIT_APPLY_CHECK, int: 1, default: 0),
               "GIT_APPLY_CHECK has type int");
_Static_assert(GIT_BRANCH_LOCAL == 1 && GIT_APPLY_CHECK == 1,
               "representative enumerator values match the pinned header");
