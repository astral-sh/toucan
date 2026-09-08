/* Include the pinned sqlite3.h before this file. */

typedef void (*toucan_sqlite_dl_symbol)(void);
typedef toucan_sqlite_dl_symbol (*toucan_sqlite_dl_lookup)(
    sqlite3_vfs *, void *, const char *);
typedef void (*toucan_sqlite_bindgen_dl_symbol)(
    sqlite3_vfs *, void *, const char *);
typedef toucan_sqlite_bindgen_dl_symbol (*toucan_sqlite_bindgen_dl_lookup)(
    sqlite3_vfs *, void *, const char *);

_Static_assert(
    __builtin_types_compatible_p(
        __typeof__(((sqlite3_vfs *)0)->xDlSym), toucan_sqlite_dl_lookup),
    "xDlSym returns a function pointer with no parameters");
_Static_assert(
    !__builtin_types_compatible_p(
        __typeof__(((sqlite3_vfs *)0)->xDlSym),
        toucan_sqlite_bindgen_dl_lookup),
    "xDlSym does not return a function pointer with its own three parameters");

_Static_assert(
    __builtin_types_compatible_p(
        __typeof__(&sqlite3_version[0]), const char *),
    "sqlite3_version has const elements");
_Static_assert(
    !__builtin_types_compatible_p(__typeof__(&sqlite3_version[0]), char *),
    "sqlite3_version does not have mutable elements");
_Static_assert(
    __builtin_types_compatible_p(
        __typeof__(&sqlite3_version), const char (*)[]),
    "the version array retains its element qualifier");
