use std::io;
use std::path::{Path, PathBuf};

use carapace::host::{ActionSpec, Host, Row, Value};
use carapace::state::StateValue;

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

const FB_ACTIONS: &[ActionSpec] = &[
    ActionSpec { name: "open_entry" },
    ActionSpec {
        name: "open_shortcut",
    },
];

/// What an `entries` row navigates to. Kept in lockstep with `rows("entries")`.
enum Target {
    Up,
    Dir(PathBuf),
    File,
}

pub struct FileBrowserHost<F: FileSystem> {
    fs: F,
    root: PathBuf,
    current: PathBuf,
    shortcuts: Vec<(String, PathBuf)>,
}

impl<F: FileSystem> FileBrowserHost<F> {
    pub fn new(fs: F, root: PathBuf, shortcuts: Vec<(String, PathBuf)>) -> Self {
        Self {
            current: root.clone(),
            fs,
            root,
            shortcuts,
        }
    }

    /// Directory entries, dirs first then files, each case-insensitively by name.
    fn sorted_entries(&self) -> Vec<DirEntryInfo> {
        let mut v = self.fs.read_dir(&self.current).unwrap_or_default();
        v.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        v
    }

    /// The (display row, navigation target) pairs for the current directory, in order.
    /// `rows("entries")` and `invoke("open_entry", i)` both derive from this, so indices align.
    fn entry_rows(&self) -> Vec<(Row, Target)> {
        let mut out = Vec::new();
        if self.current != self.root {
            out.push((
                Row::new()
                    .set("name", StateValue::Str("..".into()))
                    .set("size", StateValue::Str("".into())),
                Target::Up,
            ));
        }
        for e in self.sorted_entries() {
            let (size, target) = if e.is_dir {
                ("<dir>".to_string(), Target::Dir(self.current.join(&e.name)))
            } else {
                (human_size(e.size), Target::File)
            };
            out.push((
                Row::new()
                    .set("name", StateValue::Str(e.name.as_str().into()))
                    .set("size", StateValue::Str(size.as_str().into())),
                target,
            ));
        }
        out
    }

    /// Only allow navigating to paths at or under `root`.
    fn within_root(&self, p: &Path) -> bool {
        p.starts_with(&self.root)
    }
}

fn human_size(bytes: u64) -> String {
    const K: f64 = 1024.0;
    let b = bytes as f64;
    if b < K {
        format!("{bytes}B")
    } else if b < K * K {
        format!("{:.1}K", b / K)
    } else {
        format!("{:.1}M", b / (K * K))
    }
}

impl<F: FileSystem> Host for FileBrowserHost<F> {
    fn name(&self) -> &str {
        "file-browser"
    }
    fn tick(&mut self, _dt: std::time::Duration) {}
    fn get(&self, key: &str) -> Option<StateValue> {
        match key {
            "current_path" => Some(StateValue::Str(
                self.current.to_string_lossy().as_ref().into(),
            )),
            _ => None,
        }
    }
    fn actions(&self) -> &[ActionSpec] {
        FB_ACTIONS
    }
    fn invoke(&mut self, action: &str, args: &[Value]) {
        let index = match args.first() {
            Some(Value::Num(n)) if *n >= 0.0 => *n as usize,
            _ => return,
        };
        match action {
            "open_entry" => {
                let targets = self.entry_rows();
                match targets.into_iter().nth(index).map(|(_, t)| t) {
                    Some(Target::Up) => {
                        if let Some(parent) = self.current.parent()
                            && self.within_root(parent)
                        {
                            self.current = parent.to_path_buf();
                        }
                    }
                    Some(Target::Dir(p)) if self.within_root(&p) => {
                        self.current = p;
                    }
                    _ => {}
                }
            }
            "open_shortcut" => {
                if let Some((_, path)) = self.shortcuts.get(index)
                    && self.within_root(path)
                {
                    self.current = path.clone();
                }
            }
            _ => {}
        }
    }
    fn rows(&self, collection: &str) -> Vec<Row> {
        match collection {
            "shortcuts" => self
                .shortcuts
                .iter()
                .map(|(label, _)| Row::new().set("label", StateValue::Str(label.as_str().into())))
                .collect(),
            "entries" => self.entry_rows().into_iter().map(|(row, _)| row).collect(),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use carapace::host::{Host, Value};
    use carapace::state::StateValue;

    fn s(v: &str) -> StateValue {
        StateValue::Str(v.into())
    }

    fn fixture() -> FileBrowserHost<MockFs> {
        let fs = MockFs::new()
            .dir(
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
            )
            .dir(
                "/root/sub",
                vec![DirEntryInfo {
                    name: "b.txt".into(),
                    is_dir: false,
                    size: 512,
                }],
            );
        FileBrowserHost::new(
            fs,
            PathBuf::from("/root"),
            vec![
                ("Root".into(), PathBuf::from("/root")),
                ("Sub".into(), PathBuf::from("/root/sub")),
            ],
        )
    }

    #[test]
    fn entries_list_dirs_first_no_dotdot_at_root() {
        let h = fixture();
        let rows = h.rows("entries");
        assert_eq!(rows[0].get("name"), Some(&s("sub")));
        assert_eq!(rows[0].get("size"), Some(&s("<dir>")));
        assert_eq!(rows[1].get("name"), Some(&s("a.txt")));
        assert_eq!(rows[1].get("size"), Some(&s("2.0K")));
        assert!(
            !rows.iter().any(|r| r.get("name") == Some(&s(".."))),
            "no .. at root"
        );
    }

    #[test]
    fn shortcuts_list_labels() {
        let h = fixture();
        let rows = h.rows("shortcuts");
        assert_eq!(rows[0].get("label"), Some(&s("Root")));
        assert_eq!(rows[1].get("label"), Some(&s("Sub")));
    }

    #[test]
    fn open_entry_enters_dir_then_dotdot_goes_up() {
        let mut h = fixture();
        h.invoke("open_entry", &[Value::Num(0.0)]); // enter "sub"
        assert_eq!(h.get("current_path"), Some(s("/root/sub")));
        let rows = h.rows("entries");
        assert_eq!(rows[0].get("name"), Some(&s("..")), ".. offered below root");
        assert_eq!(rows[1].get("name"), Some(&s("b.txt")));

        h.invoke("open_entry", &[Value::Num(0.0)]); // ".." back up
        assert_eq!(h.get("current_path"), Some(s("/root")));
    }

    #[test]
    fn open_shortcut_jumps_and_stays_within_root() {
        let mut h = fixture();
        h.invoke("open_shortcut", &[Value::Num(1.0)]);
        assert_eq!(h.get("current_path"), Some(s("/root/sub")));
    }

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
