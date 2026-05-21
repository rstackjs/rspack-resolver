//! Path Utilities
//!
//! Code adapted from the following libraries
//! * [path-absolutize](https://docs.rs/path-absolutize)
//! * [normalize_path](https://docs.rs/normalize-path)

use crate::resolver_path::{Component, ResolverPath, ResolverPathBuf};

pub const SLASH_START: &[char; 2] = &['/', '\\'];

/// Extension trait that adds path-normalization helpers operating on `&str`.
///
/// Path normalization is fully implemented over the crate's
/// [`crate::resolver_path::ResolverPath`] mirror — no `std::path::PathBuf` /
/// `OsString` roundtrips. Inputs and outputs cross the API boundary as
/// `str` / `String`.
pub trait PathUtil {
  /// Normalize this path without performing I/O.
  fn normalize(&self) -> String;

  /// Normalize with subpath assuming this path is normalized without performing I/O.
  fn normalize_with(&self, subpath: &str) -> String;

  /// Defined in ESM PACKAGE_TARGET_RESOLVE
  /// If target split on "/" or "\" contains any "", ".", "..", or "node_modules" segments
  /// after the first "." segment, case insensitive and including percent encoded variants.
  fn is_invalid_exports_target(&self) -> bool;
}

impl PathUtil for str {
  fn normalize(&self) -> String {
    normalize_path(ResolverPath::new(self)).into_string()
  }

  fn normalize_with(&self, subpath: &str) -> String {
    normalize_path_with(ResolverPath::new(self), ResolverPath::new(subpath)).into_string()
  }

  fn is_invalid_exports_target(&self) -> bool {
    ResolverPath::new(self)
      .components()
      .enumerate()
      .any(|(index, c)| match c {
        Component::ParentDir => true,
        Component::CurDir => index > 0,
        Component::Normal(c) => c.eq_ignore_ascii_case("node_modules"),
        _ => false,
      })
  }
}

impl PathUtil for String {
  fn normalize(&self) -> String {
    self.as_str().normalize()
  }
  fn normalize_with(&self, subpath: &str) -> String {
    self.as_str().normalize_with(subpath)
  }
  fn is_invalid_exports_target(&self) -> bool {
    self.as_str().is_invalid_exports_target()
  }
}

impl PathUtil for ResolverPathBuf {
  fn normalize(&self) -> String {
    self.as_str().normalize()
  }
  fn normalize_with(&self, subpath: &str) -> String {
    self.as_str().normalize_with(subpath)
  }
  fn is_invalid_exports_target(&self) -> bool {
    self.as_str().is_invalid_exports_target()
  }
}

// Adapted from https://github.com/parcel-bundler/parcel/blob/e0b99c2a42e9109a9ecbd6f537844a1b33e7faf5/packages/utils/node-resolver-rs/src/path.rs#L7
//
// On Windows, `/` and `\` are both valid separators. The old `std::path::PathBuf`
// implementation preserved input separators verbatim and only added
// `MAIN_SEPARATOR` at boundaries when pushing. Many downstream callers and
// tests rely on that. Component-rebuild collapses every separator into
// `MAIN_SEPARATOR`, so we only enter the slow path when the input actually
// has something to collapse (`.`, `..`, or duplicate separators).
fn normalize_path(path: ResolverPath<'_>) -> ResolverPathBuf {
  let raw = path.as_str();
  if !needs_normalization(raw) {
    return path.to_path_buf();
  }

  let plen = prefix_byte_len(raw);
  // Original root separator byte (if any), used so a path like
  // `/foo/.././foo/` normalizes back to `/foo` even on Windows where
  // `MAIN_SEPARATOR` is `\` — std::path::Path used to preserve the
  // input separator and downstream callers expect the same.
  let root_sep = raw
    .as_bytes()
    .get(plen)
    .copied()
    .filter(|&b| matches!(b, b'/' | b'\\'))
    .map(|b| b as char);

  let mut components = path.components().peekable();
  let mut ret = if let Some(Component::Prefix(p)) = components.peek().copied() {
    components.next();
    ResolverPathBuf::from(p)
  } else {
    ResolverPathBuf::new()
  };

  let mut pending_root = false;
  for component in components {
    match component {
      Component::Prefix(_) => unreachable!("Path {:?}", path),
      Component::RootDir => {
        pending_root = true;
      }
      Component::CurDir => {}
      Component::ParentDir => {
        if !ret.pop() && !pending_root {
          ret.push("..");
        }
      }
      Component::Normal(c) => {
        if pending_root && ret.as_str().is_empty() {
          if let Some(sep) = root_sep {
            ret.push_separator_char(sep);
          } else {
            ret.push_separator();
          }
        }
        pending_root = false;
        ret.push(c);
      }
    }
  }

  ret
}

// Quick scan that returns true when the path contains `.`/`..` segments or
// duplicate separators that the slow path would collapse. Skips the Windows
// prefix (drive letter / UNC / verbatim) so the leading `\\` of a UNC share
// is not treated as a duplicate.
fn needs_normalization(s: &str) -> bool {
  let bytes = s.as_bytes();
  let plen = prefix_byte_len(s);
  let body = &bytes[plen..];
  if body.is_empty() {
    return false;
  }

  let mut i = if is_sep(body[0]) {
    if body.len() >= 2 && is_sep(body[1]) {
      return true;
    }
    1
  } else {
    0
  };

  let mut seg_start = i;
  while i < body.len() {
    let b = body[i];
    if is_sep(b) {
      let seg = &body[seg_start..i];
      if seg == b"." || seg == b".." {
        return true;
      }
      if i + 1 < body.len() && is_sep(body[i + 1]) {
        return true;
      }
      if i + 1 == body.len() {
        return true;
      }
      seg_start = i + 1;
    }
    i += 1;
  }
  let tail = &body[seg_start..];
  tail == b"." || tail == b".."
}

#[cfg(not(windows))]
#[inline]
fn prefix_byte_len(_s: &str) -> usize {
  0
}

#[cfg(windows)]
fn prefix_byte_len(s: &str) -> usize {
  ResolverPath::new(s)
    .components()
    .next()
    .and_then(|c| match c {
      Component::Prefix(p) => Some(p.len()),
      _ => None,
    })
    .unwrap_or(0)
}

#[cfg(windows)]
#[inline]
fn is_sep(b: u8) -> bool {
  b == b'/' || b == b'\\'
}
#[cfg(not(windows))]
#[inline]
fn is_sep(b: u8) -> bool {
  b == b'/'
}

// Adapted from https://github.com/parcel-bundler/parcel/blob/e0b99c2a42e9109a9ecbd6f537844a1b33e7faf5/packages/utils/node-resolver-rs/src/path.rs#L37
fn normalize_path_with(base: ResolverPath<'_>, subpath: ResolverPath<'_>) -> ResolverPathBuf {
  let mut components = subpath.components();

  let Some(head) = components.next() else {
    return subpath.to_path_buf();
  };

  if matches!(head, Component::Prefix(_) | Component::RootDir) {
    return subpath.to_path_buf();
  }

  let mut ret = base.to_path_buf();
  for component in std::iter::once(head).chain(components) {
    match component {
      Component::CurDir => {}
      Component::ParentDir => {
        ret.pop();
      }
      Component::Normal(c) => {
        ret.push(c);
      }
      Component::Prefix(_) | Component::RootDir => {
        unreachable!("Path {:?} Subpath {:?}", base, subpath)
      }
    }
  }

  ret
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
    assert!(case.is_invalid_exports_target(), "{case}");
  }

  assert!(!"C:".is_invalid_exports_target());
  assert!(!"/".is_invalid_exports_target());
}

#[tokio::test]
async fn normalize() {
  assert_eq!("/foo/.././foo/".normalize(), "/foo");
  // `Path::eq` is component-wise so the original test treated "C://" and "C:" as equal.
  // Now that we return rendered strings, the redundant trailing separators collapse.
  assert_eq!("C://".normalize(), "C:");
  assert_eq!("C:".normalize(), "C:");
  assert_eq!(r"\\server\share".normalize(), r"\\server\share");
}
