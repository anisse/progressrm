use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Component, Path, PathBuf},
};

// Copy pasted from std so we don't have to rely on unstable feature:
// https://github.com/rust-lang/rust/issues/134694
/// An error returned from [`Path::normalize_lexically`] if a `..` parent reference
/// would escape the path.
#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub struct NormalizeError;
/// Normalize a path, including `..` without traversing the filesystem.
///
/// Returns an error if normalization would leave leading `..` components.
///
/// <div class="warning">
///
/// This function always resolves `..` to the "lexical" parent.
/// That is "a/b/../c" will always resolve to `a/c` which can change the meaning of the path.
/// In particular, `a/c` and `a/b/../c` are distinct on many systems because `b` may be a symbolic link, so its parent isn’t `a`.
///
/// </div>
///
/// [`path::absolute`](absolute) is an alternative that preserves `..`.
/// Or [`Path::canonicalize`] can be used to resolve any `..` by querying the filesystem.
pub fn normalize_lexically(p: &Path) -> Result<PathBuf, NormalizeError> {
    let mut lexical = PathBuf::new();
    let mut iter = p.components().peekable();

    // Find the root, if any, and add it to the lexical path.
    // Here we treat the Windows path "C:\" as a single "root" even though
    // `components` splits it into two: (Prefix, RootDir).
    let root = match iter.peek() {
        Some(Component::ParentDir) => return Err(NormalizeError),
        Some(p @ Component::RootDir) | Some(p @ Component::CurDir) => {
            lexical.push(p);
            iter.next();
            lexical.as_os_str().len()
        }
        Some(Component::Prefix(prefix)) => {
            lexical.push(prefix.as_os_str());
            iter.next();
            if let Some(p @ Component::RootDir) = iter.peek() {
                lexical.push(p);
                iter.next();
            }
            lexical.as_os_str().len()
        }
        None => return Ok(PathBuf::new()),
        Some(Component::Normal(_)) => 0,
    };

    for component in iter {
        match component {
            Component::RootDir => unreachable!(),
            Component::Prefix(_) => return Err(NormalizeError),
            Component::CurDir => continue,
            Component::ParentDir => {
                // It's an error if ParentDir causes us to go above the "root".
                if lexical.as_os_str().len() == root {
                    return Err(NormalizeError);
                } else {
                    lexical.pop();
                }
            }
            Component::Normal(path) => lexical.push(path),
        }
    }
    Ok(lexical)
}

struct PidIterator {
    pids: Box<dyn Iterator<Item = u32>>,
}
impl PidIterator {
    pub fn new(process_match: &'static str) -> Result<Self, String> {
        Ok(Self {
            pids: Box::new(
                fs::read_dir("/proc")
                    .map_err(|e| format!("opening /proc: {e}"))?
                    .filter_map(|res| res.ok()) // discard errors for individual files
                    .map(|f| f.file_name().into_string()) // keep only basename from path, and only Strings
                    .filter_map(|res| res.ok()) // valid utf-8 only
                    .filter(|f| f.as_bytes().iter().all(|c| c.is_ascii_digit())) // only pids (digit-only strings)
                    .filter_map(|f| f.parse::<u32>().ok()) // as integers
                    .filter(move |pid| {
                        if let Ok(path) = fs::read_link(format!("/proc/{pid}/exe"))
                            && let Some(s) = path.to_str()
                        {
                            return s.contains(process_match);
                        }
                        false
                    }), // only process which exe matches pattern
            ),
        })
    }
}
impl Iterator for PidIterator {
    type Item = u32;
    fn next(&mut self) -> Option<Self::Item> {
        self.pids.next()
    }
}

pub struct FdIterator {
    fds: fs::ReadDir,
}

impl FdIterator {
    pub fn new(pid: u32) -> Result<Self, String> {
        Ok(FdIterator {
            fds: fs::read_dir(format!("/proc/{pid}/fd"))
                .map_err(|e| format!("cannot open fd dir: {e}"))?,
        })
    }
}
impl Iterator for FdIterator {
    type Item = PathBuf;
    fn next(&mut self) -> Option<Self::Item> {
        for fd in (&mut self.fds).filter_map(|res| res.ok()) {
            if let Ok(link) = fs::read_link(fd.path())
                && let Some(s) = link.to_str()
                && s.starts_with("/")
            {
                return Some(link);
            }
        }
        None
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for pid in PidIterator::new("/usr/bin/rm")? {
        let cwd: PathBuf = fs::read_link(format!("/proc/{pid}/cwd"))?;
        // Parse cmdline
        let cmdline: Vec<PathBuf> = fs::read_to_string(format!("/proc/{pid}/cmdline"))?
            .split_terminator('\0')
            .filter(|s| !s.starts_with("--")) // Normally this filter should not be effective after --
            .map(|s| normalize_lexically(&cwd.join(s)).expect("normalizable path"))
            .collect();
        let lookup_hash: HashMap<PathBuf, usize> = cmdline
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, el)| (el, i))
            .collect();
        for filename in FdIterator::new(pid)? {
            println!("{pid}: {filename:?}");
            // Go up the tree until we find it
            let mut components = filename.components();
            loop {
                let p = components.as_path().to_owned();
                if let Some(i) = lookup_hash.get(&p) {
                    println!("Progress: {i} / {}", cmdline.len());
                    break;
                }
                let end = components.next_back();
                if end.is_none() {
                    break;
                }
            }
        }
    }
    Ok(())
}
