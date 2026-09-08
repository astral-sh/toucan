use std::sync::atomic::{AtomicUsize, Ordering};

use rusqlite::functions::FunctionFlags;
use rusqlite::trace::TraceEventCodes;
use rusqlite::{ffi, params, Connection, MAIN_DB};

static EXTENSIONS: AtomicUsize = AtomicUsize::new(0);
static STATEMENTS: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn extension(
    db: *mut ffi::sqlite3,
    _error: *mut *mut core::ffi::c_char,
    _api: *const ffi::sqlite3_api_routines,
) -> core::ffi::c_int {
    if db.is_null() {
        return ffi::SQLITE_ERROR;
    }
    EXTENSIONS.fetch_add(1, Ordering::Relaxed);
    ffi::SQLITE_OK
}

fn main() -> rusqlite::Result<()> {
    // The callback only records registration and returns SQLite's success code.
    unsafe { rusqlite::auto_extension::register_auto_extension(extension)? };
    let db = Connection::open_in_memory()?;
    assert_eq!(EXTENSIONS.load(Ordering::Relaxed), 1);
    assert!(rusqlite::auto_extension::cancel_auto_extension(extension));
    db.trace_v2(
        TraceEventCodes::SQLITE_TRACE_STMT,
        Some(|_| {
            STATEMENTS.fetch_add(1, Ordering::Relaxed);
        }),
    );
    db.create_scalar_function("twice", 1, FunctionFlags::SQLITE_DETERMINISTIC, |context| {
        Ok(context.get::<i64>(0)? * 2)
    })?;
    db.execute_batch("CREATE TABLE values_to_test (id INTEGER, text_value TEXT)")?;
    for value in 0..32 {
        db.execute(
            "INSERT INTO values_to_test VALUES (?1, ?2)",
            params![value, format!("row {value}")],
        )?;
    }
    let sum: i64 = db.query_row("SELECT sum(twice(id)) FROM values_to_test", [], |row| {
        row.get(0)
    })?;
    assert_eq!(sum, 992);
    let text: String = db.query_row(
        "SELECT text_value FROM values_to_test WHERE id = ?1",
        [17],
        |row| row.get(0),
    )?;
    assert_eq!(text, "row 17");
    assert!(STATEMENTS.load(Ordering::Relaxed) >= 35);
    let serialized = db.serialize(MAIN_DB)?;
    let mut restored = Connection::open_in_memory()?;
    restored.deserialize_read_exact(MAIN_DB, serialized.as_ref(), serialized.len(), false)?;
    let count: i64 =
        restored.query_row("SELECT count(*) FROM values_to_test", [], |row| row.get(0))?;
    assert_eq!(count, 32);
    assert_eq!(EXTENSIONS.load(Ordering::Relaxed), 1);
    // Exercise the sys crate's public CStr constant against the linked C library.
    let version = unsafe { core::ffi::CStr::from_ptr(ffi::sqlite3_libversion()) };
    assert_eq!(ffi::SQLITE_VERSION, version);
    println!("SQLite {}: 32 rows, prepared parameters, scalar/trace/extension callbacks, {} serialized bytes restored", version.to_str().unwrap(), serialized.len());
    Ok(())
}
