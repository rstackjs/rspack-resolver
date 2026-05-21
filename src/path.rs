//! Path Utilities
//!
//! Code adapted from the following libraries
//! * [path-absolutize](https://docs.rs/path-absolutize)
//! * [normalize_path](https://docs.rs/normalize-path)
use std::path::{Component, Path};

pub const SLASH_START: &[char; 2] = &['/', '\\'];

/// Extension trait that adds path-normalization helpers operating on `&str`.
///
/// Path normalization still leans on [`std::path::Path`] to handle platform
/// quirks (drive prefixes, UNC, parent traversal), but every input and output
/// crosses the API boundary as `str`/`String`.
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
    normalize_path(Path::new(self))
  }

  fn normalize_with(&self, subpath: &str) -> String {
    normalize_path_with(Path::new(self), Path::new(subpath))
  }

  fn is_invalid_exports_target(&self) -> bool {
    Path::new(self)
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

impl PathUtil for crate::str_path::StrPath {
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
fn normalize_path(path: &Path) -> String {
  use std::path::PathBuf;
  let mut components = path.components().peekable();
  let mut ret = if let Some(c @ Component::Prefix(..)) = components.peek() {
    let buf = PathBuf::from(c.as_os_str());
    components.next();
    buf
  } else {
    PathBuf::new()
  };

  for component in components {
    match component {
      Component::Prefix(..) => unreachable!("Path {:?}", path),
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

  ret.to_str().expect("path should be UTF-8").to_string()
}

// Adapted from https://github.com/parcel-bundler/parcel/blob/e0b99c2a42e9109a9ecbd6f537844a1b33e7faf5/packages/utils/node-resolver-rs/src/path.rs#L37
fn normalize_path_with(base: &Path, subpath: &Path) -> String {
  let mut components = subpath.components();

  let Some(head) = components.next() else {
    return subpath.to_str().expect("path should be UTF-8").to_string();
  };

  if matches!(head, Component::Prefix(..) | Component::RootDir) {
    return subpath.to_str().expect("path should be UTF-8").to_string();
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
      Component::Prefix(..) | Component::RootDir => {
        unreachable!("Path {:?} Subpath {:?}", base, subpath)
      }
    }
  }

  ret.to_str().expect("path should be UTF-8").to_string()
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
