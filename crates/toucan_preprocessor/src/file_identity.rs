//! Host file identity for multiply-linked headers, from the handle used to read them.

use std::fs::File;
use std::io;

/// Ordinary files use canonical paths and need no entry in the hard-link index.
#[cfg(unix)]
pub(crate) fn linked_identity(file: &File) -> io::Result<Option<(u64, u64)>> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok((metadata.nlink() > 1).then(|| (metadata.dev(), metadata.ino())))
}

/// Do not coalesce Windows files using a 64-bit index that can collide on ReFS.
#[cfg(windows)]
pub(crate) fn linked_identity(file: &File) -> io::Result<Option<(u64, u64)>> {
    let information = winapi_util::file::information(file)?;
    if information.number_of_links() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows headers with multiple or unknown link counts require a supported 128-bit file identity API",
        ));
    }
    Ok(None)
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn linked_identity(_file: &File) -> io::Result<Option<(u64, u64)>> {
    Ok(None)
}
