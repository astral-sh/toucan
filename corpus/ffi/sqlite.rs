fn ffi_test() {
    unsafe {
        assert_eq!(b::sqlite3_libversion_number(), 3045001);
        let mut database = std::ptr::null_mut();
        assert_eq!(
            b::sqlite3_open(c":memory:".as_ptr(), &mut database),
            b::SQLITE_OK
        );
        assert!(!database.is_null());
        let mut statement = std::ptr::null_mut();
        let sql = c"SELECT ?1 + 2, ?2";
        assert_eq!(
            b::sqlite3_prepare_v2(
                database,
                sql.as_ptr(),
                -1,
                &mut statement,
                std::ptr::null_mut()
            ),
            b::SQLITE_OK
        );
        assert_eq!(b::sqlite3_bind_int(statement, 1, 40), b::SQLITE_OK);
        let text = c"toucan";
        // A null destructor means SQLITE_STATIC; this CString lives through finalization.
        assert_eq!(
            b::sqlite3_bind_text(statement, 2, text.as_ptr(), -1, None),
            b::SQLITE_OK
        );
        assert_eq!(b::sqlite3_step(statement), b::SQLITE_ROW);
        assert_eq!(b::sqlite3_column_int(statement, 0), 42);
        assert_eq!(
            std::ffi::CStr::from_ptr(b::sqlite3_column_text(statement, 1).cast()).to_bytes(),
            b"toucan"
        );
        assert_eq!(b::sqlite3_step(statement), b::SQLITE_DONE);
        assert_eq!(b::sqlite3_finalize(statement), b::SQLITE_OK);
        assert_eq!(b::sqlite3_close(database), b::SQLITE_OK);
    }
}
