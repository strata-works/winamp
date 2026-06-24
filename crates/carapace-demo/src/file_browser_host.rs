use std::io;
use std::path::Path;

/// One directory entry, filesystem-agnostic.
#[derive(Clone, Debug, PartialEq)]
pub struct DirEntryInfo {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// Read-only directory listing. Abstracted so tests use an in-memory tree.
pub trait FileSystem {
    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntryInfo>>;
}

/// The real, read-only filesystem.
pub struct StdFs;
impl FileSystem for StdFs {
    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntryInfo>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            let is_dir = meta.is_dir();
            out.push(DirEntryInfo {
                name: entry.file_name().to_string_lossy().into_owned(),
                is_dir,
                size: if is_dir { 0 } else { meta.len() },
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
pub struct MockFs {
    dirs: std::collections::HashMap<std::path::PathBuf, Vec<DirEntryInfo>>,
}
#[cfg(test)]
impl MockFs {
    pub fn new() -> Self {
        Self {
            dirs: std::collections::HashMap::new(),
        }
    }
    pub fn dir(mut self, path: &str, entries: Vec<DirEntryInfo>) -> Self {
        self.dirs.insert(std::path::PathBuf::from(path), entries);
        self
    }
}
#[cfg(test)]
impl FileSystem for MockFs {
    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntryInfo>> {
        self.dirs
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such mock dir"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn mockfs_returns_seeded_entries() {
        let fs = MockFs::new().dir(
            "/root",
            vec![
                DirEntryInfo {
                    name: "sub".into(),
                    is_dir: true,
                    size: 0,
                },
                DirEntryInfo {
                    name: "a.txt".into(),
                    is_dir: false,
                    size: 2048,
                },
            ],
        );
        let entries = fs.read_dir(&PathBuf::from("/root")).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(
            entries
                .iter()
                .any(|e| e.name == "a.txt" && !e.is_dir && e.size == 2048)
        );
    }

    #[test]
    fn mockfs_unknown_dir_errors() {
        let fs = MockFs::new();
        assert!(fs.read_dir(&PathBuf::from("/nope")).is_err());
    }

    #[test]
    fn stdfs_reads_a_real_directory() {
        let fs = StdFs;
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let entries = fs.read_dir(dir).unwrap();
        assert!(
            entries.iter().any(|e| e.name == "Cargo.toml" && !e.is_dir),
            "demo crate dir contains a Cargo.toml file"
        );
    }
}
