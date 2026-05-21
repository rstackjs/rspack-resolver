use std::{fmt, sync::Arc};

use crate::{
  package_json::PackageJson,
  resolver_path::{ResolverPath, ResolverPathBuf},
};

/// The final path resolution with optional `?query` and `#fragment`.
///
/// The resolved path is stored as [`ResolverPathBuf`] so it carries the
/// precomputed FxHash that the resolver computed on the way out. Callers
/// can read the hash via [`Resolution::resolver_path`] / [`Resolution::path_buf`]
/// to skip a re-hash on insertion into their own caches.
#[derive(Clone)]
pub struct Resolution {
  pub(crate) path: ResolverPathBuf,

  /// path query `?query`, contains `?`.
  pub(crate) query: Option<String>,

  /// path fragment `#query`, contains `#`.
  pub(crate) fragment: Option<String>,

  pub(crate) package_json: Option<Arc<PackageJson>>,
}

impl fmt::Debug for Resolution {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Resolution")
      .field("path", &self.path)
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
  /// Returns the path without query and fragment, as a `&str` (unchanged API).
  pub fn path(&self) -> &str {
    self.path.as_str()
  }

  /// Returns the path as the in-crate [`ResolverPath`] view — same data as
  /// [`Resolution::path`] but with a precomputed FxHash attached. Use this if
  /// you intend to feed the path into a hash map / set in the consumer.
  pub fn resolver_path(&self) -> ResolverPath<'_> {
    self.path.as_path()
  }

  /// Returns the owned [`ResolverPathBuf`] (consuming the resolution) —
  /// carries the precomputed hash so a `Cache<ResolverPathBuf, V>` lookup
  /// won't have to re-hash the bytes.
  pub fn into_path_buf(self) -> ResolverPathBuf {
    self.path
  }

  /// Returns the path without query and fragment as an owned `String`.
  /// Convenience for callers that don't care about the precomputed hash.
  pub fn into_path(self) -> String {
    self.path.into_string()
  }

  /// Returns the path query `?query`, contains the leading `?`
  pub fn query(&self) -> Option<&str> {
    self.query.as_deref()
  }

  /// Returns the path fragment `#fragment`, contains the leading `#`
  pub fn fragment(&self) -> Option<&str> {
    self.fragment.as_deref()
  }

  /// Returns serialized package_json
  pub fn package_json(&self) -> Option<&Arc<PackageJson>> {
    self.package_json.as_ref()
  }

  /// Returns the full path with query and fragment
  pub fn full_path(&self) -> String {
    let mut path = self.path.as_str().to_string();
    if let Some(query) = &self.query {
      path.push_str(query);
    }
    if let Some(fragment) = &self.fragment {
      path.push_str(fragment);
    }
    path
  }
}

#[tokio::test]
async fn test() {
  let resolution = Resolution {
    path: ResolverPathBuf::from("foo"),
    query: Some("?query".to_string()),
    fragment: Some("#fragment".to_string()),
    package_json: None,
  };
  assert_eq!(resolution.path(), "foo");
  assert_eq!(resolution.query(), Some("?query"));
  assert_eq!(resolution.fragment(), Some("#fragment"));
  assert_eq!(resolution.full_path(), "foo?query#fragment");
  // resolver_path() forwards the same prehash that path_buf carries.
  let rp = resolution.resolver_path();
  assert_eq!(rp.as_str(), "foo");
  assert_eq!(rp.precomputed_hash(), resolution.path.precomputed_hash());
  assert_eq!(resolution.into_path(), "foo");
}
