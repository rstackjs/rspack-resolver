use std::{
  borrow::{Borrow, Cow},
  convert::AsRef,
  fmt,
  future::Future,
  hash::{BuildHasherDefault, Hash, Hasher},
  io,
  ops::Deref,
  path::{Path, PathBuf},
  sync::Arc,
};

use dashmap::{DashMap, DashSet};
use futures::future::BoxFuture;
use rustc_hash::FxHasher;
use tokio::sync::OnceCell as OnceLock;

use crate::{
  context::ResolveContext as Ctx,
  package_json::{off_to_location, PackageJson},
  path::{join_str, parent_str, PathUtil},
  FileMetadata, FileSystem, JSONError, ResolveError, ResolveOptions, TsConfig,
};

#[derive(Default)]
pub struct Cache<Fs> {
  pub(crate) fs: Fs,
  paths: DashSet<ResolverPath, BuildHasherDefault<IdentityHasher>>,
  tsconfigs: DashMap<PathBuf, Arc<TsConfig>, BuildHasherDefault<FxHasher>>,
}

impl<Fs: Send + Sync + FileSystem> Cache<Fs> {
  pub fn new(fs: Fs) -> Self {
    Self {
      fs,
      paths: DashSet::default(),
      tsconfigs: DashMap::default(),
    }
  }

  pub fn clear(&self) {
    self.paths.clear();
    self.tsconfigs.clear();
  }

  /// Look up or create a [`ResolverPath`]. Errors if `path` is not valid UTF-8.
  pub fn value<P: AsRef<Path>>(&self, path: P) -> Result<ResolverPath, ResolveError> {
    let path = path.as_ref();
    let s = path
      .to_str()
      .ok_or_else(|| ResolveError::PathNotUtf8(path.to_path_buf()))?;
    Ok(self.value_str(s))
  }

  /// Internal hot-path entry. Caller guarantees UTF-8.
  pub(crate) fn value_str(&self, s: &str) -> ResolverPath {
    let hash = fx_hash_bytes(s.as_bytes());
    if let Some(entry) = self.paths.get((hash, s).borrow() as &dyn CacheKey) {
      return entry.clone();
    }
    let parent = parent_str(s).map(|p| self.value_str(p));
    let inner = ResolverPathInner::new(hash, s.into(), parent);
    let rp = ResolverPath(Arc::new(inner));
    self.paths.insert(rp.clone());
    rp
  }

  pub async fn tsconfig<F, Fut>(
    &self,
    root: bool,
    path: &Path,
    callback: F, // callback for modifying tsconfig with `extends`
  ) -> Result<Arc<TsConfig>, ResolveError>
  where
    F: FnOnce(TsConfig) -> Fut + Send,
    Fut: Send + Future<Output = Result<TsConfig, ResolveError>>,
  {
    if let Some(tsconfig_ref) = self.tsconfigs.get(path) {
      return Ok(Arc::clone(tsconfig_ref.value()));
    }
    let meta = self.fs.metadata(path).await.ok();
    let tsconfig_path = if meta.is_some_and(|m| m.is_file) {
      Cow::Borrowed(path)
    } else if meta.is_some_and(|m| m.is_dir) {
      Cow::Owned(path.join("tsconfig.json"))
    } else {
      let mut os_string = path.to_path_buf().into_os_string();
      os_string.push(".json");
      Cow::Owned(PathBuf::from(os_string))
    };
    let mut tsconfig_string = self
      .fs
      .read_to_string(&tsconfig_path)
      .await
      .map_err(|_| ResolveError::TsconfigNotFound(path.to_path_buf()))?;
    let mut tsconfig =
      TsConfig::parse(root, &tsconfig_path, &mut tsconfig_string).map_err(|error| {
        ResolveError::from_serde_json_error(
          tsconfig_path.to_path_buf(),
          &error,
          Some(tsconfig_string),
        )
      })?;
    tsconfig = callback(tsconfig).await?;
    let tsconfig = Arc::new(tsconfig.build());
    self
      .tsconfigs
      .insert(path.to_path_buf(), Arc::clone(&tsconfig));
    Ok(tsconfig)
  }
}

#[inline]
fn fx_hash_bytes(bytes: &[u8]) -> u64 {
  let mut hasher = FxHasher::default();
  hasher.write(bytes);
  hasher.finish()
}

/// Unified path value carrying a precomputed `FxHasher` u64 and a UTF-8 path
/// string. Cloning is one `Arc` bump; the cache hands back the same instance
/// it stores internally.
#[derive(Clone)]
pub struct ResolverPath(pub(crate) Arc<ResolverPathInner>);

impl ResolverPath {
  #[inline]
  pub fn path(&self) -> &Path {
    Path::new(self.0.path.as_ref())
  }

  #[inline]
  pub fn as_str(&self) -> &str {
    &self.0.path
  }

  /// Precomputed FxHasher u64 over the path bytes — downstream may reuse this
  /// directly when storing `ResolverPath` in an FxHashMap.
  #[inline]
  pub fn hash(&self) -> u64 {
    self.0.hash
  }

  #[inline]
  pub fn parent(&self) -> Option<&Self> {
    self.0.parent.as_ref()
  }
}

impl Hash for ResolverPath {
  fn hash<H: Hasher>(&self, state: &mut H) {
    state.write_u64(self.0.hash);
  }
}

impl PartialEq for ResolverPath {
  fn eq(&self, other: &Self) -> bool {
    self.0.path == other.0.path
  }
}
impl Eq for ResolverPath {}

impl Deref for ResolverPath {
  type Target = ResolverPathInner;
  fn deref(&self) -> &Self::Target {
    self.0.as_ref()
  }
}

impl AsRef<ResolverPathInner> for ResolverPath {
  fn as_ref(&self) -> &ResolverPathInner {
    self.0.as_ref()
  }
}

impl AsRef<Path> for ResolverPath {
  fn as_ref(&self) -> &Path {
    self.path()
  }
}

impl Borrow<str> for ResolverPath {
  fn borrow(&self) -> &str {
    self.as_str()
  }
}

impl fmt::Debug for ResolverPath {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_tuple("ResolverPath").field(&self.as_str()).finish()
  }
}

impl fmt::Display for ResolverPath {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

impl<'a> Borrow<dyn CacheKey + 'a> for ResolverPath {
  fn borrow(&self) -> &(dyn CacheKey + 'a) {
    self
  }
}

impl CacheKey for ResolverPath {
  fn tuple(&self) -> (u64, &str) {
    (self.0.hash, self.as_str())
  }
}

impl ResolverPath {
  /// Construct a standalone [`ResolverPath`] from a path — does NOT touch the
  /// cache. Used for resolver results (e.g. the canonicalized realpath) that
  /// the caller needs as a typed `ResolverPath` but should not pollute the
  /// resolver cache with.
  ///
  /// The returned value has `parent: None` and empty fs OnceCells. Cheap to
  /// allocate (one `Arc<ResolverPathInner>`) and intentionally not stored
  /// anywhere — drop it when done.
  ///
  /// # Errors
  ///
  /// Returns [`ResolveError::PathNotUtf8`] if `path` is not valid UTF-8.
  pub(crate) fn shallow<P: AsRef<Path>>(path: P) -> Result<Self, ResolveError> {
    let path = path.as_ref();
    let s = path
      .to_str()
      .ok_or_else(|| ResolveError::PathNotUtf8(path.to_path_buf()))?;
    let hash = fx_hash_bytes(s.as_bytes());
    Ok(Self(Arc::new(ResolverPathInner::new(hash, s.into(), None))))
  }
}

#[cfg(test)]
impl ResolverPath {
  /// Construct a `ResolverPath` directly from a UTF-8 string — test-only.
  #[doc(hidden)]
  pub fn for_test(s: &str) -> Self {
    let hash = fx_hash_bytes(s.as_bytes());
    let inner = ResolverPathInner::new(hash, s.into(), None);
    ResolverPath(Arc::new(inner))
  }
}

pub struct ResolverPathInner {
  hash: u64,
  path: Box<str>,
  parent: Option<ResolverPath>,
  meta: OnceLock<Option<FileMetadata>>,
  canonicalized: OnceLock<Option<PathBuf>>,
  node_modules: OnceLock<Option<ResolverPath>>,
  package_json: OnceLock<Option<Arc<PackageJson>>>,
}

impl ResolverPathInner {
  fn new(hash: u64, path: Box<str>, parent: Option<ResolverPath>) -> Self {
    Self {
      hash,
      path,
      parent,
      meta: OnceLock::new(),
      canonicalized: OnceLock::new(),
      node_modules: OnceLock::new(),
      package_json: OnceLock::new(),
    }
  }

  pub fn path(&self) -> &Path {
    Path::new(self.path.as_ref())
  }

  pub fn as_str(&self) -> &str {
    &self.path
  }

  pub fn to_path_buf(&self) -> PathBuf {
    PathBuf::from(self.path.as_ref())
  }

  pub fn parent(&self) -> Option<&ResolverPath> {
    self.parent.as_ref()
  }

  async fn meta<Fs: Send + Sync + FileSystem>(&self, fs: &Fs) -> Option<FileMetadata> {
    // Skip the Future state-machine + poll on cache hit. `tokio::sync::OnceCell::get`
    // is sync and bypasses constructing the `get_or_init` future entirely.
    if let Some(m) = self.meta.get() {
      return *m;
    }
    *self
      .meta
      .get_or_init(|| async { fs.metadata(self.path()).await.ok() })
      .await
  }

  pub async fn is_file<Fs: Send + Sync + FileSystem>(&self, fs: &Fs, ctx: &mut Ctx) -> bool {
    if let Some(meta) = self.meta(fs).await {
      ctx.add_file_dependency(self.path());
      meta.is_file
    } else {
      ctx.add_missing_dependency(self.path());
      false
    }
  }

  pub async fn is_dir<Fs: Send + Sync + FileSystem>(&self, fs: &Fs, ctx: &mut Ctx) -> bool {
    self.meta(fs).await.map_or_else(
      || {
        ctx.add_missing_dependency(self.path());
        false
      },
      |meta| meta.is_dir,
    )
  }

  pub fn realpath<'a, Fs: FileSystem + Send + Sync>(
    &'a self,
    fs: &'a Fs,
  ) -> BoxFuture<'a, io::Result<PathBuf>> {
    let fut = async move {
      // Cache hit: return immediately and avoid the recursive parent walk +
      // `Box::pin` per call on the hot path.
      if let Some(cached) = self.canonicalized.get() {
        return Ok(cached.clone().unwrap_or_else(|| self.to_path_buf()));
      }
      self
        .canonicalized
        .get_or_try_init(|| async move {
          if fs
            .symlink_metadata(self.path())
            .await
            .is_ok_and(|m| m.is_symlink)
          {
            return fs.canonicalize(self.path()).await.map(Some);
          }
          if let Some(parent) = self.parent() {
            let parent_path = parent.realpath(fs).await?;
            return Ok(Some(
              parent_path.normalize_with(self.path().strip_prefix(parent.path()).unwrap()),
            ));
          }
          Ok(None)
        })
        .await
        .cloned()
        .map(|r| r.unwrap_or_else(|| self.to_path_buf()))
    };
    Box::pin(fut)
  }

  pub async fn module_directory<Fs: Send + Sync + FileSystem>(
    &self,
    module_name: &str,
    cache: &Cache<Fs>,
    ctx: &mut Ctx,
  ) -> Option<ResolverPath> {
    let joined = join_str(&self.path, module_name);
    let cached_path = cache.value_str(&joined);
    cached_path
      .is_dir(&cache.fs, ctx)
      .await
      .then_some(cached_path)
  }

  pub async fn cached_node_modules<Fs: Send + Sync + FileSystem>(
    &self,
    cache: &Cache<Fs>,
    ctx: &mut Ctx,
  ) -> Option<ResolverPath> {
    if let Some(nm) = self.node_modules.get() {
      return nm.clone();
    }
    self
      .node_modules
      .get_or_init(|| self.module_directory("node_modules", cache, ctx))
      .await
      .clone()
  }

  /// Find package.json of a path by traversing parent directories.
  ///
  /// # Errors
  ///
  /// * [ResolveError::JSON]
  #[cfg_attr(feature="enable_instrument", tracing::instrument(level=tracing::Level::DEBUG, skip_all, fields(path = %self.path().display())))]
  pub async fn find_package_json<Fs: FileSystem + Send + Sync>(
    &self,
    fs: &Fs,
    options: &ResolveOptions,
    ctx: &mut Ctx,
  ) -> Result<Option<Arc<PackageJson>>, ResolveError> {
    let mut cache_value = self;
    // Go up directories when the querying path is not a directory
    while !cache_value.is_dir(fs, ctx).await {
      if let Some(cv) = &cache_value.parent {
        cache_value = cv.as_ref();
      } else {
        break;
      }
    }
    let mut cache_value = Some(cache_value);
    while let Some(cv) = cache_value {
      if let Some(package_json) = cv.package_json(fs, options, ctx).await? {
        return Ok(Some(Arc::clone(&package_json)));
      }
      cache_value = cv.parent.as_deref();
    }
    Ok(None)
  }

  /// Get package.json of the given path.
  ///
  /// # Errors
  ///
  /// * [ResolveError::JSON]
  #[cfg_attr(feature="enable_instrument", tracing::instrument(level=tracing::Level::DEBUG, skip_all, fields(path = %self.path().display())))]
  pub async fn package_json<Fs: FileSystem + Send + Sync>(
    &self,
    fs: &Fs,
    options: &ResolveOptions,
    ctx: &mut Ctx,
  ) -> Result<Option<Arc<PackageJson>>, ResolveError> {
    if let Some(pkg) = self.package_json.get() {
      // Preserve ctx dependency tracking on cache hit.
      match pkg {
        Some(package_json) => ctx.add_file_dependency(&package_json.path),
        None => {
          if let Some(deps) = &mut ctx.missing_dependencies {
            deps.push(self.path().join("package.json"));
          }
        }
      }
      return Ok(pkg.clone());
    }
    // Change to `std::sync::OnceLock::get_or_try_init` when it is stable.
    let result = self
      .package_json
      .get_or_try_init(|| async {
        let package_json_path = self.path().join("package.json");
        let Ok(package_json_string) = fs.read(&package_json_path).await else {
          return Ok(None);
        };
        let real_path = if options.symlinks {
          self.realpath(fs).await?.join("package.json")
        } else {
          package_json_path.clone()
        };
        match PackageJson::parse(package_json_path.clone(), real_path, package_json_string) {
          Ok(v) => Ok(Some(Arc::new(v))),
          Err(parse_err) => {
            let package_json_path = self.path().join("package.json");
            let package_json_string = match fs.read_to_string(&package_json_path).await {
              Ok(c) => c,
              Err(io_err) => {
                return Err(ResolveError::from(io_err));
              }
            };
            let serde_err = serde_json::from_str::<serde_json::Value>(&package_json_string).err();

            if let Some(err) = serde_err {
              Err(ResolveError::from_serde_json_error(
                package_json_path,
                &err,
                Some(package_json_string),
              ))
            } else {
              let (line, column) = off_to_location(&package_json_string, parse_err.index());

              Err(ResolveError::JSON(JSONError {
                path: package_json_path,
                message: parse_err.error().to_string(),
                line,
                column,
                content: Some(package_json_string),
              }))
            }
          }
        }
      })
      .await
      .cloned();

    // https://github.com/webpack/enhanced-resolve/blob/58464fc7cb56673c9aa849e68e6300239601e615/lib/DescriptionFileUtils.js#L68-L82
    match &result {
      Ok(Some(package_json)) => {
        ctx.add_file_dependency(&package_json.path);
      }
      Ok(None) => {
        // Avoid an allocation by making this lazy
        if let Some(deps) = &mut ctx.missing_dependencies {
          deps.push(self.path().join("package.json"));
        }
      }
      Err(_) => {
        if let Some(deps) = &mut ctx.file_dependencies {
          deps.push(self.path().join("package.json"));
        }
      }
    }
    result
  }
}

/// Memoized cache key, code adapted from <https://stackoverflow.com/a/50478038>.
trait CacheKey {
  fn tuple(&self) -> (u64, &str);
}

impl Hash for dyn CacheKey + '_ {
  fn hash<H: Hasher>(&self, state: &mut H) {
    state.write_u64(self.tuple().0);
  }
}

impl PartialEq for dyn CacheKey + '_ {
  fn eq(&self, other: &Self) -> bool {
    self.tuple().1 == other.tuple().1
  }
}

impl Eq for dyn CacheKey + '_ {}

impl CacheKey for (u64, &str) {
  fn tuple(&self) -> (u64, &str) {
    (self.0, self.1)
  }
}

impl<'a> Borrow<dyn CacheKey + 'a> for (u64, &'a str) {
  fn borrow(&self) -> &(dyn CacheKey + 'a) {
    self
  }
}

/// Since the cache key is memoized, use an identity hasher
/// to avoid double cache.
#[derive(Default)]
struct IdentityHasher(u64);

impl Hasher for IdentityHasher {
  fn write(&mut self, _: &[u8]) {
    unreachable!("Invalid use of IdentityHasher")
  }
  fn write_u64(&mut self, n: u64) {
    self.0 = n;
  }
  fn finish(&self) -> u64 {
    self.0
  }
}

#[cfg(test)]
mod cache_path_tests {
  use super::*;
  use crate::FileSystemOs;

  fn fresh_cache() -> Cache<FileSystemOs> {
    Cache::new(FileSystemOs::default())
  }

  #[test]
  fn resolver_path_hash_matches_fx_of_bytes() {
    let cache = fresh_cache();
    let rp = cache.value("/a/b/c").expect("utf8");
    let mut h = FxHasher::default();
    h.write(b"/a/b/c");
    assert_eq!(rp.hash(), h.finish());
  }

  #[test]
  fn equal_paths_share_arc() {
    let cache = fresh_cache();
    let a = cache.value("/foo/bar").expect("utf8");
    let b = cache.value("/foo/bar").expect("utf8");
    assert!(Arc::ptr_eq(&a.0, &b.0));
  }

  #[test]
  fn parent_chain_walks_cache() {
    let cache = fresh_cache();
    let child = cache.value("/foo/bar").expect("utf8");
    let parent_via_chain = child.parent().expect("has parent").clone();
    let parent_via_lookup = cache.value("/foo").expect("utf8");
    assert!(Arc::ptr_eq(&parent_via_chain.0, &parent_via_lookup.0));
  }

  #[cfg(unix)]
  #[test]
  fn non_utf8_path_errors() {
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
    let cache = fresh_cache();
    let bytes = [0xff, 0xfe, 0x2f, 0x61];
    let p = PathBuf::from(OsStr::from_bytes(&bytes));
    let err = cache.value(&p).unwrap_err();
    assert!(matches!(err, ResolveError::PathNotUtf8(_)), "got {err:?}");
  }

  #[test]
  fn as_str_and_path_view() {
    let cache = fresh_cache();
    let rp = cache.value("/abc/def").expect("utf8");
    assert_eq!(rp.as_str(), "/abc/def");
    assert_eq!(rp.path(), Path::new("/abc/def"));
  }
}
