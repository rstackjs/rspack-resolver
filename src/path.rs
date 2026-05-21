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

/// `Path::join` equivalent that allocates the worst-case capacity up front so
/// the inner `Vec` never has to regrow when the separator + segment is pushed.
/// Hot on parent walks that repeatedly join names like `node_modules` and
/// `package.json` to a cached directory path.
#[inline]
pub fn path_join_preallocated(base: &Path, sub: &str) -> PathBuf {
  let mut buf = PathBuf::with_capacity(base.as_os_str().len() + sub.len() + 1);
  buf.push(base);
  buf.push(sub);
  buf
}

/// Byte-level [`Path::parent`] for unix targets.
///
/// `std::path::Path::parent` builds a `Components` iterator and walks one step
/// back, which the bench shows costs ~2M Ir per `resolver/single-thread`
/// iteration (one call per cache miss in [`crate::cache::Cache::value`]).
/// Operating on raw bytes — like the existing `Cache::value` hash and the
/// `CachedPath` eq path — sidesteps that.
///
/// Behavior mirrors std's impl: drops trailing separators, splits at the last
/// remaining separator, collapses repeated separators before the split point,
/// and returns `None` for an empty path or a path whose only content is one
/// or more separators (i.e. root).
#[cfg(unix)]
#[inline]
pub fn path_parent_unix(path: &Path) -> Option<&Path> {
  use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

  let bytes = path.as_os_str().as_bytes();
  let last_non_slash = bytes.iter().rposition(|&b| b != b'/')?;
  let trimmed = &bytes[..=last_non_slash];
  let parent_end = trimmed
    .iter()
    .rposition(|&b| b == b'/')
    .map_or(0, |slash_pos| {
      bytes[..slash_pos]
        .iter()
        .rposition(|&b| b != b'/')
        .map_or_else(|| usize::from(bytes.first() == Some(&b'/')), |p| p + 1)
    });
  Some(Path::new(OsStr::from_bytes(&bytes[..parent_end])))
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

    // Pre-size to the worst-case length so the loop's pushes can never grow
    // the inner Vec. `+1` covers the separator inserted by `PathBuf::push`.
    let mut ret = PathBuf::with_capacity(self.as_os_str().len() + subpath.as_os_str().len() + 1);
    ret.push(self);
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

#[cfg(unix)]
#[tokio::test]
async fn path_parent_unix_matches_std() {
  let cases = [
    "/foo/bar",
    "/foo",
    "/",
    "",
    "foo",
    "foo/bar",
    ".",
    "..",
    "./foo",
    "../foo",
    "/foo/",
    "/foo/bar/",
    "//foo/bar",
    "foo//bar",
    "/a/b/c/d/e",
    "/",
    "//",
  ];
  for case in cases {
    let p = Path::new(case);
    assert_eq!(
      path_parent_unix(p),
      p.parent(),
      "case={case:?} diverged from std::Path::parent"
    );
  }
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
