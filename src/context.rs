use std::{
  ops::{Deref, DerefMut},
  path::{Path, PathBuf},
};

use crate::{cache::CachedPath, error::ResolveError};

#[derive(Debug, Default, Clone)]
pub struct ResolveContext(ResolveContextImpl);

#[derive(Debug, Default, Clone)]
pub struct ResolveContextImpl {
  pub fully_specified: bool,

  pub query: Option<String>,

  pub fragment: Option<String>,

  /// Files that was found on file system
  pub file_dependencies: Option<Vec<PathBuf>>,

  /// Files that was found on file system
  pub missing_dependencies: Option<Vec<PathBuf>>,

  /// The current resolving alias for bailing recursion alias.
  pub resolving_alias: Option<String>,

  /// Stack-based recursion detection (ported from enhanced-resolve).
  /// A duplicate `(path, specifier)` entry means we have entered a cycle.
  /// Entries are unwound via `finish_resolve` so sibling fallback branches
  /// don't see stale entries from earlier attempts.
  resolve_stack: Vec<(CachedPath, String)>,

  /// Depth guard for non-repeating rewrite cycles where the specifier
  /// changes on every hop (e.g. alias expansion `a → b/c → a/c/c → …`).
  depth: u8,
}

impl Deref for ResolveContext {
  type Target = ResolveContextImpl;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl DerefMut for ResolveContext {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.0
  }
}

impl ResolveContext {
  pub fn with_fully_specified(&mut self, yes: bool) {
    self.fully_specified = yes;
  }

  pub fn with_query_fragment(&mut self, query: Option<&str>, fragment: Option<&str>) {
    if let Some(query) = query {
      self.query.replace(query.to_string());
    }
    if let Some(fragment) = fragment {
      self.fragment.replace(fragment.to_string());
    }
  }

  pub fn init_file_dependencies(&mut self) {
    self.file_dependencies.replace(vec![]);
    self.missing_dependencies.replace(vec![]);
  }

  pub fn add_file_dependency(&mut self, dep: &Path) {
    if let Some(deps) = &mut self.file_dependencies {
      deps.push(dep.to_path_buf());
    }
  }

  pub fn add_missing_dependency(&mut self, dep: &Path) {
    if let Some(deps) = &mut self.missing_dependencies {
      deps.push(dep.to_path_buf());
    }
  }

  pub fn with_resolving_alias(&mut self, alias: String) {
    self.resolving_alias = Some(alias);
  }

  /// enhanced-resolve: Resolver.doResolve stack-based recursion detection.
  ///
  /// 1. Checks whether the same `(path, specifier)` pair already exists in
  ///    the resolve stack — a duplicate means a repeating cycle.
  /// 2. Enforces a depth limit to catch non-repeating rewrite cycles where
  ///    the specifier changes on every hop.
  pub fn test_for_infinite_recursion(
    &mut self,
    cached_path: &CachedPath,
    specifier: &str,
  ) -> Result<(), ResolveError> {
    if self
      .resolve_stack
      .iter()
      .any(|(p, s)| p == cached_path && s == specifier)
    {
      return Err(ResolveError::Recursion);
    }

    self.depth += 1;
    if self.depth > 32 {
      return Err(ResolveError::Recursion);
    }

    self
      .resolve_stack
      .push((cached_path.clone(), specifier.to_string()));
    Ok(())
  }

  pub fn finish_resolve(&mut self) {
    // just pop stack, DO NOT decrease depth to keep depth detection unchanged.
    self.resolve_stack.pop();
  }
}
