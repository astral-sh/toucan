// These callback signatures match libsqlite3-sys's build.rs overrides.
// SQLite's C header uses void (*)(void) to accept extension entry points;
// SQLite calls those entry points with the arguments below.
unsafe extern "C" {
    pub fn sqlite3_auto_extension(
        entry: Option<unsafe extern "C" fn(
            db: *mut sqlite3,
            error: *mut *mut ::core::ffi::c_char,
            api: *const sqlite3_api_routines,
        ) -> ::core::ffi::c_int>,
    ) -> ::core::ffi::c_int;
    pub fn sqlite3_cancel_auto_extension(
        entry: Option<unsafe extern "C" fn(
            db: *mut sqlite3,
            error: *mut *mut ::core::ffi::c_char,
            api: *const sqlite3_api_routines,
        ) -> ::core::ffi::c_int>,
    ) -> ::core::ffi::c_int;
}
