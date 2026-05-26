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
    // Why: On Unix, an `OsStr` is raw bytes and `/`, `.` are always single-byte ASCII
    // regardless of UTF-8 content in segments. Iterating bytes directly skips
    // the heavy `Components` state machine (`parse_next_component_back`,
    // `Component::PartialEq`, double-ended iter bookkeeping) that dominated
    // ~3% of the resolver's instructions in callgrind.
    #[cfg(unix)]
    {
      unix_normalize(self)
    }
    #[cfg(not(unix))]
    {
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
  }

  // https://github.com/parcel-bundler/parcel/blob/e0b99c2a42e9109a9ecbd6f537844a1b33e7faf5/packages/utils/node-resolver-rs/src/path.rs#L37
  fn normalize_with<B: AsRef<Self>>(&self, subpath: B) -> PathBuf {
    let subpath = subpath.as_ref();

    // Why: callgrind showed `Components::next` + `parse_next_component_back` +
    // `Component::PartialEq` totalling ~5% of Ir, almost all driven from
    // `normalize_with` calls in the resolver hot path. On Unix the separator
    // and `.`/`..` markers are guaranteed single-byte ASCII, so a byte-level
    // pass produces identical output without the iterator overhead.
    #[cfg(unix)]
    {
      unix_normalize_with(self, subpath)
    }
    #[cfg(not(unix))]
    {
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

/// Byte-level `normalize` for Unix. See [`PathUtil::normalize`] for why.
#[cfg(unix)]
fn unix_normalize(path: &Path) -> PathBuf {
  use std::{
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
  };

  let bytes = path.as_os_str().as_bytes();
  let leading_slash = bytes.first() == Some(&b'/');

  // Worst-case capacity: original length + a trailing slash placeholder.
  let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
  if leading_slash {
    out.push(b'/');
  }

  // Track segment offsets we've written into `out` so `..` can pop in O(1)
  // instead of rescanning `out` byte-by-byte.
  let mut starts: Vec<usize> = Vec::new();

  for seg in bytes.split(|&b| b == b'/') {
    match seg {
      b"" | b"." => {}
      b".." => {
        if let Some(start) = starts.pop() {
          // Trim trailing `/` left over from a previous segment.
          out.truncate(start.saturating_sub(usize::from(start > usize::from(leading_slash))));
        }
      }
      normal => {
        // Insert a separator before every segment except the very first one
        // when there is no leading slash.
        if out.len() > usize::from(leading_slash) {
          out.push(b'/');
        }
        starts.push(out.len());
        out.extend_from_slice(normal);
      }
    }
  }

  if out.is_empty() {
    return PathBuf::new();
  }

  PathBuf::from(OsString::from_vec(out))
}

/// Byte-level `normalize_with` for Unix. See [`PathUtil::normalize_with`] for why.
#[cfg(unix)]
fn unix_normalize_with(base: &Path, subpath: &Path) -> PathBuf {
  use std::{
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
  };

  let sub_bytes = subpath.as_os_str().as_bytes();

  if sub_bytes.is_empty() {
    return subpath.to_path_buf();
  }

  // Absolute subpath short-circuits to subpath, matching the std behavior of
  // `PathBuf::push` and the original Components-based implementation.
  if sub_bytes[0] == b'/' {
    return subpath.to_path_buf();
  }

  let base_bytes = base.as_os_str().as_bytes();
  let mut out: Vec<u8> = Vec::with_capacity(base_bytes.len() + 1 + sub_bytes.len());
  out.extend_from_slice(base_bytes);

  for seg in sub_bytes.split(|&b| b == b'/') {
    match seg {
      b"" | b"." => {}
      b".." => {
        // Pop the trailing segment from `out` without rescanning whole bytes
        // ahead of time: `rposition` walks from the end.
        if let Some(slash) = out.iter().rposition(|&b| b == b'/') {
          if slash == 0 {
            out.truncate(1);
          } else {
            out.truncate(slash);
          }
        } else {
          out.clear();
        }
      }
      normal => {
        if !out.is_empty() && *out.last().unwrap() != b'/' {
          out.push(b'/');
        }
        out.extend_from_slice(normal);
      }
    }
  }

  PathBuf::from(OsString::from_vec(out))
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
