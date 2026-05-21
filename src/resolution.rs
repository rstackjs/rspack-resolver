use std::{
  fmt,
  path::{Path, PathBuf},
  sync::Arc,
};

use crate::{cache::ResolverPath, package_json::PackageJson};

/// The final path resolution with optional `?query` and `#fragment`.
#[derive(Clone)]
pub struct Resolution {
  pub(crate) path: ResolverPath,

  /// path query `?query`, contains `?`.
  pub(crate) query: Option<String>,

  /// path fragment `#query`, contains `#`.
  pub(crate) fragment: Option<String>,

  pub(crate) package_json: Option<Arc<PackageJson>>,
}

impl fmt::Debug for Resolution {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Resolution")
      .field("path", &self.path.path())
      .field("query", &self.query)
      .field("fragment", &self.fragment)
      .field("package_json", &self.package_json.as_ref().map(|p| &p.path))
      .finish()
  }
}

impl PartialEq for Resolution {
  fn eq(&self, other: &Self) -> bool {
    self.path == other.path && self.query == other.query && self.fragment == other.fragment
  }
}
impl Eq for Resolution {}

impl Resolution {
  /// Returns the path without query and fragment.
  pub fn path(&self) -> &Path {
    self.path.path()
  }

  /// Returns the path without query and fragment.
  pub fn into_path_buf(self) -> PathBuf {
    self.path.path().to_path_buf()
  }

  /// Returns the path query `?query`, contains the leading `?`.
  pub fn query(&self) -> Option<&str> {
    self.query.as_deref()
  }

  /// Returns the path fragment `#fragment`, contains the leading `#`.
  pub fn fragment(&self) -> Option<&str> {
    self.fragment.as_deref()
  }

  /// Returns serialized package_json.
  pub fn package_json(&self) -> Option<&Arc<PackageJson>> {
    self.package_json.as_ref()
  }

  /// Returns the full path with query and fragment.
  pub fn full_path(&self) -> PathBuf {
    let mut path = self.path.path().to_path_buf().into_os_string();
    if let Some(query) = &self.query {
      path.push(query);
    }
    if let Some(fragment) = &self.fragment {
      path.push(fragment);
    }
    PathBuf::from(path)
  }

  /// Borrow the underlying [`ResolverPath`], which carries a precomputed
  /// FxHasher u64 and a UTF-8 `&str` view. Downstream may reuse this directly
  /// to avoid re-hashing the path.
  pub fn resolver_path(&self) -> &ResolverPath {
    &self.path
  }

  /// Move out the underlying [`ResolverPath`] (cheap `Arc` clone semantics).
  pub fn into_resolver_path(self) -> ResolverPath {
    self.path
  }
}

#[tokio::test]
async fn test() {
  let resolution = Resolution {
    path: ResolverPath::for_test("foo"),
    query: Some("?query".to_string()),
    fragment: Some("#fragment".to_string()),
    package_json: None,
  };
  assert_eq!(resolution.path(), Path::new("foo"));
  assert_eq!(resolution.query(), Some("?query"));
  assert_eq!(resolution.fragment(), Some("#fragment"));
  assert_eq!(resolution.full_path(), PathBuf::from("foo?query#fragment"));
  assert_eq!(resolution.into_path_buf(), PathBuf::from("foo"));
}

#[tokio::test]
async fn resolver_path_exposes_hash() {
  use std::env;

  use crate::{ResolveOptions, Resolver};

  let dir = env::current_dir().expect("cwd");
  let resolver = Resolver::new(ResolveOptions::default());
  let res = resolver
    .resolve(&dir, "./Cargo.toml")
    .await
    .expect("resolve");

  let rp = res.resolver_path();
  let mut h = rustc_hash::FxHasher::default();
  use std::hash::Hasher;
  h.write(rp.as_str().as_bytes());
  assert_eq!(rp.hash(), h.finish());
  assert_eq!(rp.path(), res.path());
}
