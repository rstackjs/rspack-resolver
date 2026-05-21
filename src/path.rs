//! Path Utilities
//!
//! Code adapted from the following libraries
//! * [path-absolutize](https://docs.rs/path-absolutize)
//! * [normalize_path](https://docs.rs/normalize-path)
use std::path::{Component, Path, PathBuf};

pub const SLASH_START: &[char; 2] = &['/', '\\'];

pub fn path_to_str(path: &Path) -> &str {
  path.to_str().expect("path should be UTF-8")
}

/// Strip a single trailing path separator (platform-aware). Used so that
/// `parent_str("/a/b/")` walks to `"/a"` instead of `"/a/b"`.
#[inline]
fn trim_one_trailing_sep(s: &str) -> &str {
  #[cfg(windows)]
  {
    s.strip_suffix('\\')
      .or_else(|| s.strip_suffix('/'))
      .unwrap_or(s)
  }
  #[cfg(not(windows))]
  {
    s.strip_suffix('/').unwrap_or(s)
  }
}

#[inline]
fn last_sep_index(s: &str) -> Option<usize> {
  #[cfg(windows)]
  {
    match (s.rfind('/'), s.rfind('\\')) {
      (Some(a), Some(b)) => Some(a.max(b)),
      (Some(a), None) | (None, Some(a)) => Some(a),
      (None, None) => None,
    }
  }
  #[cfg(not(windows))]
  {
    s.rfind('/')
  }
}

/// Return the parent slice of a UTF-8 path string.
///
/// Splits on the last `/` (and `\` on Windows). Returns `None` when the path
/// has no parent (Unix root `/`, Windows drive root `C:\`, or a relative single
/// component).
#[inline]
pub fn parent_str(s: &str) -> Option<&str> {
  if s.is_empty() {
    return None;
  }
  let trimmed = trim_one_trailing_sep(s);
  let idx = last_sep_index(trimmed)?;
  let prefix = &trimmed[..idx];
  let include_sep =
    prefix.is_empty() || (cfg!(windows) && prefix.len() == 2 && prefix.ends_with(':'));
  if include_sep {
    Some(&trimmed[..=idx])
  } else {
    Some(prefix)
  }
}

/// Join a UTF-8 base path with a single segment using the platform separator.
///
/// Mirrors `std::path::Path::join` semantics: when `segment` is absolute
/// (starts with `/`, or `\` on Windows) it replaces `base` entirely. Otherwise
/// the segment is appended, inserting a separator only when `base` does not
/// already end in one.
#[inline]
pub fn join_str(base: &str, segment: &str) -> String {
  if Path::new(segment).is_absolute() {
    return segment.to_string();
  }
  let already_sep =
    base.ends_with(std::path::MAIN_SEPARATOR) || (cfg!(windows) && base.ends_with('/'));
  let extra = usize::from(!already_sep);
  let mut out = String::with_capacity(base.len() + extra + segment.len());
  out.push_str(base);
  if !already_sep {
    out.push(std::path::MAIN_SEPARATOR);
  }
  out.push_str(segment);
  out
}

/// Extension trait to add path normalization to std's [`Path`].
pub trait PathUtil {
  /// Normalize this path without performing I/O.
  ///
  /// All redundant separator and up-level references are collapsed.
  ///
  /// However, this does not resolve links.
  fn normalize(&self) -> PathBuf;

  /// Normalize with subpath assuming this path is normalized without performing I/O.
  ///
  /// All redundant separator and up-level references are collapsed.
  ///
  /// However, this does not resolve links.
  fn normalize_with<P: AsRef<Path>>(&self, subpath: P) -> PathBuf;

  /// Defined in ESM PACKAGE_TARGET_RESOLVE
  /// If target split on "/" or "\" contains any "", ".", "..", or "node_modules" segments after the first "." segment, case insensitive and including percent encoded variants
  fn is_invalid_exports_target(&self) -> bool;
}

impl PathUtil for Path {
  // https://github.com/parcel-bundler/parcel/blob/e0b99c2a42e9109a9ecbd6f537844a1b33e7faf5/packages/utils/node-resolver-rs/src/path.rs#L7
  fn normalize(&self) -> PathBuf {
    let mut components = self.components().peekable();
    let mut ret = if let Some(c @ Component::Prefix(..)) = components.peek() {
      let buf = PathBuf::from(c.as_os_str());
      components.next();
      buf
    } else {
      PathBuf::new()
    };

    for component in components {
      match component {
        Component::Prefix(..) => unreachable!("Path {:?}", self),
        Component::RootDir => {
          ret.push(component.as_os_str());
        }
        Component::CurDir => {}
        Component::ParentDir => {
          ret.pop();
        }
        Component::Normal(c) => {
          ret.push(c);
        }
      }
    }

    ret
  }

  // https://github.com/parcel-bundler/parcel/blob/e0b99c2a42e9109a9ecbd6f537844a1b33e7faf5/packages/utils/node-resolver-rs/src/path.rs#L37
  fn normalize_with<B: AsRef<Self>>(&self, subpath: B) -> PathBuf {
    let subpath = subpath.as_ref();

    let mut components = subpath.components();

    let Some(head) = components.next() else {
      return subpath.to_path_buf();
    };

    if matches!(head, Component::Prefix(..) | Component::RootDir) {
      return subpath.to_path_buf();
    }

    let mut ret = self.to_path_buf();
    for component in std::iter::once(head).chain(components) {
      match component {
        Component::CurDir => {}
        Component::ParentDir => {
          ret.pop();
        }
        Component::Normal(c) => {
          ret.push(c);
        }
        Component::Prefix(..) | Component::RootDir => {
          unreachable!("Path {:?} Subpath {:?}", self, subpath)
        }
      }
    }

    ret
  }

  fn is_invalid_exports_target(&self) -> bool {
    self.components().enumerate().any(|(index, c)| match c {
      Component::ParentDir => true,
      Component::CurDir => index > 0,
      Component::Normal(c) => c.eq_ignore_ascii_case("node_modules"),
      _ => false,
    })
  }
}

// https://github.com/webpack/enhanced-resolve/blob/main/test/path.test.js
#[tokio::test]
async fn is_invalid_exports_target() {
  let test_cases = [
    "../a.js",
    "../",
    "./a/b/../../../c.js",
    "./a/b/../../../",
    "./../../c.js",
    "./../../",
    "./a/../b/../../c.js",
    "./a/../b/../../",
    "./././../",
  ];

  for case in test_cases {
    assert!(Path::new(case).is_invalid_exports_target(), "{case}");
  }

  assert!(!Path::new("C:").is_invalid_exports_target());
  assert!(!Path::new("/").is_invalid_exports_target());
}

#[tokio::test]
async fn normalize() {
  assert_eq!(Path::new("/foo/.././foo/").normalize(), Path::new("/foo"));
  assert_eq!(Path::new("C://").normalize(), Path::new("C://"));
  assert_eq!(Path::new("C:").normalize(), Path::new("C:"));
  assert_eq!(
    Path::new(r"\\server\share").normalize(),
    Path::new(r"\\server\share")
  );
}

#[test]
fn parent_str_handles_common_cases() {
  assert_eq!(parent_str("/a/b/c"), Some("/a/b"));
  assert_eq!(parent_str("/a/b"), Some("/a"));
  assert_eq!(parent_str("/a"), Some("/"));
  assert_eq!(parent_str("/"), None);
  assert_eq!(parent_str("a"), None);
  assert_eq!(parent_str(""), None);
  assert_eq!(parent_str("/a/b/"), Some("/a"));
}

#[cfg(windows)]
#[test]
fn parent_str_handles_windows_separators() {
  assert_eq!(parent_str(r"C:\a\b\c"), Some(r"C:\a\b"));
  assert_eq!(parent_str(r"C:\a"), Some(r"C:\"));
  assert_eq!(parent_str(r"C:\"), None);
}

#[test]
fn join_str_appends_with_separator() {
  let sep = std::path::MAIN_SEPARATOR;
  assert_eq!(join_str("/a/b", "c"), format!("/a/b{sep}c"));
  assert_eq!(join_str("/", "c"), "/c");
  assert_eq!(join_str(&format!("/a{sep}"), "b"), format!("/a{sep}b"));
}

#[test]
fn join_str_absolute_segment_replaces_base() {
  // Mirror Path::join semantics: an absolute segment replaces the base.
  assert_eq!(join_str("/a/b", "/modules"), "/modules");
  assert_eq!(join_str("/x", "/y/z"), "/y/z");
}
