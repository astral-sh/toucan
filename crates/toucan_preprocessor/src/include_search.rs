//! Per-run include-directory identity and search order.

use std::collections::BTreeSet;
use std::path::Path;

use crate::{Config, Error};

pub(crate) struct SearchOrder {
    indices: Vec<u32>,
}

#[cfg(unix)]
type DirectoryIdentity = (u64, u64);
#[cfg(not(unix))]
type DirectoryIdentity = std::path::PathBuf;

fn directory_identity(path: &Path) -> Option<DirectoryIdentity> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_dir() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some((metadata.dev(), metadata.ino()))
    }
    #[cfg(not(unix))]
    {
        std::fs::canonicalize(path).ok()
    }
}

impl SearchOrder {
    /// Resolve duplicates once for this preprocessing run. A system entry takes
    /// precedence over a regular entry; the first spelling in its group survives.
    pub(crate) fn resolve(config: &Config) -> Result<Option<Box<Self>>, Error> {
        let regular = config.include_dirs.len();
        let count = regular.saturating_add(config.system_include_dirs.len());
        if count > 65_536 {
            return Err(Error::new(
                Path::new("<include-paths>"),
                1,
                "include directory count exceeds the 65536-entry limit",
            ));
        }
        if count <= 1 || !config.allow_filesystem {
            return Ok(None);
        }
        let mut identities = BTreeSet::new();
        let mut system = Vec::new();
        for (index, path) in config.system_include_dirs.iter().enumerate() {
            if let Some(identity) = directory_identity(path)
                && identities.insert(identity)
            {
                system.push((regular + index) as u32);
            }
        }
        let mut indices = Vec::with_capacity(count);
        for (index, path) in config.include_dirs.iter().enumerate() {
            if let Some(identity) = directory_identity(path)
                && identities.insert(identity)
            {
                indices.push(index as u32);
            }
        }
        indices.extend(system);
        Ok(Some(Box::new(Self { indices })))
    }

    pub(crate) fn len(&self) -> usize {
        self.indices.len()
    }
    pub(crate) fn index(&self, position: usize) -> usize {
        self.indices[position] as usize
    }
}
