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

impl PathUtil for ResolverPath {
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
fn normalize_path(path: &ResolverPath) -> ResolverPathBuf {
  let mut components = path.components().peekable();
  let mut ret = if let Some(Component::Prefix(p)) = components.peek().copied() {
    components.next();
    ResolverPathBuf::from(p)
  } else {
    ResolverPathBuf::new()
  };

  for component in components {
    match component {
      Component::Prefix(_) => unreachable!("Path {:?}", path),
      Component::RootDir => {
        ret.push(ResolverPath::new(component.as_str()));
      }
      Component::CurDir => {}
      Component::ParentDir => {
        ret.pop();
      }
      Component::Normal(c) => {
        ret.push(ResolverPath::new(c));
      }
    }
  }

  ret
}

// Adapted from https://github.com/parcel-bundler/parcel/blob/e0b99c2a42e9109a9ecbd6f537844a1b33e7faf5/packages/utils/node-resolver-rs/src/path.rs#L37
fn normalize_path_with(base: &ResolverPath, subpath: &ResolverPath) -> ResolverPathBuf {
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
        ret.push(ResolverPath::new(c));
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
