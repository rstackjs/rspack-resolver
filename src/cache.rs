use std::{
  borrow::{Borrow, Cow},
  convert::AsRef,
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

#[cfg(not(unix))]
use crate::path::PathUtil;
use crate::{
  context::ResolveContext as Ctx,
  package_json::{off_to_location, PackageJson},
  resolver_path::{hash_path, ResolverPath},
  FileMetadata, FileSystem, JSONError, ResolveError, ResolveOptions, TsConfig,
};

#[derive(Default)]
pub struct Cache<Fs> {
  pub(crate) fs: Fs,
  paths: DashSet<CachedPath, BuildHasherDefault<IdentityHasher>>,
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

  pub fn value(&self, path: &Path) -> CachedPath {
    let hash = hash_path(path);
    if let Some(cache_entry) = self.paths.get((hash, path).borrow() as &dyn CacheKey) {
      return cache_entry.clone();
    }
    // Why: Cache::value is the recursive parent-walk root. `Path::parent` goes
    // through `Components::next_back` / `parse_next_component_back`, which
    // callgrind showed as the single largest non-allocator non-simd-json
    // hotspot. On Unix the separator is always single-byte ASCII, so an
    // `rposition(/)` over raw `OsStr` bytes is equivalent and far cheaper.
    let parent = parent_path(path).map(|p| self.value(p));
    let data = CachedPath(Arc::new(CachedPathImpl::new(
      hash,
      path.to_path_buf().into_boxed_path(),
      parent,
    )));
    self.paths.insert(data.clone());
    data
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

#[derive(Clone)]
pub struct CachedPath(Arc<CachedPathImpl>);

impl Hash for CachedPath {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.0.hash.hash(state);
  }
}

impl PartialEq for CachedPath {
  fn eq(&self, other: &Self) -> bool {
    self.0.path == other.0.path
  }
}
impl Eq for CachedPath {}

impl Deref for CachedPath {
  type Target = CachedPathImpl;

  fn deref(&self) -> &Self::Target {
    self.0.as_ref()
  }
}

impl<'a> Borrow<dyn CacheKey + 'a> for CachedPath {
  fn borrow(&self) -> &(dyn CacheKey + 'a) {
    self
  }
}

impl AsRef<CachedPathImpl> for CachedPath {
  fn as_ref(&self) -> &CachedPathImpl {
    self.0.as_ref()
  }
}

impl CacheKey for CachedPath {
  fn tuple(&self) -> (u64, &Path) {
    (self.hash, &self.path)
  }
}

pub struct CachedPathImpl {
  hash: u64,
  path: Box<Path>,
  parent: Option<CachedPath>,
  meta: OnceLock<Option<FileMetadata>>,
  canonicalized: OnceLock<Option<PathBuf>>,
  node_modules: OnceLock<Option<CachedPath>>,
  package_json: OnceLock<Option<Arc<PackageJson>>>,
  /// Memoized `<self.path>/package.json` `ResolverPath` for the
  /// `missing_dependencies` push that fires on every `package_json` cache-hit
  /// `None` (~97% of `package_json` calls in dep-tracking workloads).
  package_json_dep_path: std::sync::OnceLock<ResolverPath>,
}

impl From<&CachedPathImpl> for ResolverPath {
  /// Reuse the cache-side `FxHash` (already computed in `Cache::value`); the
  /// only remaining work is one `Arc::from(&Path)` to materialize the shared
  /// path buffer for the `ResolveContext` sink.
  fn from(cached: &CachedPathImpl) -> Self {
    Self::from_parts(cached.hash, Arc::from(&*cached.path))
  }
}

impl CachedPathImpl {
  fn new(hash: u64, path: Box<Path>, parent: Option<CachedPath>) -> Self {
    Self {
      hash,
      path,
      parent,
      meta: OnceLock::new(),
      canonicalized: OnceLock::new(),
      node_modules: OnceLock::new(),
      package_json: OnceLock::new(),
      package_json_dep_path: std::sync::OnceLock::new(),
    }
  }

  /// Without this cache, each `None` cache-hit on `package_json` would
  /// re-`join` + re-allocate the `Arc<Path>` + re-hash on every push.
  fn package_json_dep_path(&self) -> ResolverPath {
    self
      .package_json_dep_path
      .get_or_init(|| self.path.join("package.json").into())
      .clone()
  }

  pub fn path(&self) -> &Path {
    &self.path
  }

  pub fn to_path_buf(&self) -> PathBuf {
    self.path.to_path_buf()
  }

  pub fn parent(&self) -> Option<&CachedPath> {
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
      .get_or_init(|| async { fs.metadata(&self.path).await.ok() })
      .await
  }

  pub async fn is_file<Fs: Send + Sync + FileSystem>(&self, fs: &Fs, ctx: &mut Ctx) -> bool {
    if let Some(meta) = self.meta(fs).await {
      ctx.add_file_dependency(self);
      meta.is_file
    } else {
      ctx.add_missing_dependency(self);
      false
    }
  }

  pub async fn is_dir<Fs: Send + Sync + FileSystem>(&self, fs: &Fs, ctx: &mut Ctx) -> bool {
    self.meta(fs).await.map_or_else(
      || {
        ctx.add_missing_dependency(self);
        false
      },
      |meta| meta.is_dir,
    )
  }

  pub async fn realpath<Fs: FileSystem + Send + Sync>(&self, fs: &Fs) -> io::Result<PathBuf> {
    // Cache hit: avoid the heap-allocated `Box::pin` for the cache-miss state machine
    // by returning before delegating to the boxed recursive helper.
    if let Some(cached) = self.canonicalized.get() {
      return Ok(cached.clone().unwrap_or_else(|| self.path.to_path_buf()));
    }
    self.realpath_uncached(fs).await
  }

  fn realpath_uncached<'a, Fs: FileSystem + Send + Sync>(
    &'a self,
    fs: &'a Fs,
  ) -> BoxFuture<'a, io::Result<PathBuf>> {
    Box::pin(async move {
      self
        .canonicalized
        .get_or_try_init(|| async move {
          if fs
            .symlink_metadata(&self.path)
            .await
            .is_ok_and(|m| m.is_symlink)
          {
            return fs.canonicalize(&self.path).await.map(Some);
          }
          if let Some(parent) = self.parent() {
            let parent_path = parent.realpath(fs).await?;
            // Why: parent's `path` is a strict byte prefix of `self.path`
            // (parents are produced by the byte-level `parent_path`), so
            // `strip_prefix` is the path between them. Skipping
            // `Path::strip_prefix` + `normalize_with` avoids another
            // `Components` walk per realpath miss.
            return Ok(Some(join_last_segment(
              &parent_path,
              &self.path,
              &parent.path,
            )));
          }
          Ok(None)
        })
        .await
        .cloned()
        .map(|r| r.unwrap_or_else(|| self.path.to_path_buf()))
    })
  }

  pub async fn module_directory<Fs: Send + Sync + FileSystem>(
    &self,
    module_name: &str,
    cache: &Cache<Fs>,
    ctx: &mut Ctx,
  ) -> Option<CachedPath> {
    let cached_path = cache.value(&self.path.join(module_name));
    cached_path
      .is_dir(&cache.fs, ctx)
      .await
      .then_some(cached_path)
  }

  pub async fn cached_node_modules<Fs: Send + Sync + FileSystem>(
    &self,
    cache: &Cache<Fs>,
    ctx: &mut Ctx,
  ) -> Option<CachedPath> {
    if let Some(nm) = self.node_modules.get() {
      // Replay ctx tracking from the cold path: module_directory -> is_dir calls
      // ctx.add_missing_dependency when node_modules doesn't exist on disk.
      if nm.is_none() {
        ctx.add_missing_dependency(self.path.join("node_modules"));
      }
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
  #[cfg_attr(feature="enable_instrument", tracing::instrument(level=tracing::Level::DEBUG, skip_all, fields(path = %self.path.display())))]
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
  #[cfg_attr(feature="enable_instrument", tracing::instrument(level=tracing::Level::DEBUG, skip_all, fields(path = %self.path.display())))]
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
          if ctx.missing_dependencies.is_some() {
            ctx.add_missing_dependency(self.package_json_dep_path());
          }
        }
      }
      return Ok(pkg.clone());
    }
    // Change to `std::sync::OnceLock::get_or_try_init` when it is stable.
    let result = self
      .package_json
      .get_or_try_init(|| async {
        let package_json_path = self.path.join("package.json");
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
            let package_json_path = self.path.join("package.json");
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
        if ctx.missing_dependencies.is_some() {
          ctx.add_missing_dependency(self.package_json_dep_path());
        }
      }
      Err(_) => {
        if ctx.file_dependencies.is_some() {
          ctx.add_file_dependency(self.path.join("package.json"));
        }
      }
    }
    result
  }
}

/// Join `base` with the last segment of `child`, where `child_parent` is the
/// `parent_path()` of `child` (i.e. a strict byte prefix of `child`). Used by
/// `realpath_uncached` to avoid walking `Path::strip_prefix` + `normalize_with`
/// when we already know the suffix is a single normal segment.
#[cfg(unix)]
fn join_last_segment(base: &Path, child: &Path, child_parent: &Path) -> PathBuf {
  use std::{
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
  };

  let child_bytes = child.as_os_str().as_bytes();
  let parent_len = child_parent.as_os_str().len();

  // Skip the `/` between parent and the trailing segment when applicable.
  let suffix_start = if parent_len < child_bytes.len() && child_bytes[parent_len] == b'/' {
    parent_len + 1
  } else {
    parent_len
  };
  let suffix = &child_bytes[suffix_start..];

  let base_bytes = base.as_os_str().as_bytes();
  let mut out = Vec::with_capacity(base_bytes.len() + 1 + suffix.len());
  out.extend_from_slice(base_bytes);

  if !suffix.is_empty() {
    if !out.is_empty() && *out.last().unwrap() != b'/' {
      out.push(b'/');
    }
    out.extend_from_slice(suffix);
  }

  PathBuf::from(OsString::from_vec(out))
}

#[cfg(not(unix))]
fn join_last_segment(base: &Path, child: &Path, child_parent: &Path) -> PathBuf {
  use crate::path::PathUtil;
  base.normalize_with(child.strip_prefix(child_parent).unwrap())
}

/// Byte-level parent lookup for Unix. See `Cache::value` for why.
#[cfg(unix)]
fn parent_path(path: &Path) -> Option<&Path> {
  use std::os::unix::ffi::OsStrExt;
  let bytes = path.as_os_str().as_bytes();
  // Trim a trailing `/` that isn't itself the root, mirroring std's
  // `Components` ignoring redundant separators.
  let trimmed_len = match bytes {
    [.., b'/'] if bytes.len() > 1 => bytes.len() - 1,
    _ => bytes.len(),
  };
  let trimmed = &bytes[..trimmed_len];
  let last_slash = trimmed.iter().rposition(|&b| b == b'/')?;
  if last_slash == 0 {
    // Parent is the root "/".
    if bytes.len() == 1 {
      // Path was "/", no parent.
      return None;
    }
    return Some(Path::new(std::ffi::OsStr::from_bytes(&bytes[..1])));
  }
  Some(Path::new(std::ffi::OsStr::from_bytes(&bytes[..last_slash])))
}

#[cfg(not(unix))]
#[inline]
fn parent_path(path: &Path) -> Option<&Path> {
  path.parent()
}

/// Memoized cache key, code adapted from <https://stackoverflow.com/a/50478038>.
trait CacheKey {
  fn tuple(&self) -> (u64, &Path);
}

impl Hash for dyn CacheKey + '_ {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.tuple().0.hash(state);
  }
}

impl PartialEq for dyn CacheKey + '_ {
  fn eq(&self, other: &Self) -> bool {
    self.tuple().1 == other.tuple().1
  }
}

impl Eq for dyn CacheKey + '_ {}

impl CacheKey for (u64, &Path) {
  fn tuple(&self) -> (u64, &Path) {
    (self.0, self.1)
  }
}

impl<'a> Borrow<dyn CacheKey + 'a> for (u64, &'a Path) {
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
