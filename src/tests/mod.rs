mod alias;
mod browser_field;
mod builtins;
mod dependencies;
mod exports_field;
mod extension_alias;
mod extensions;
mod fallback;
mod full_specified;
mod imports_field;
mod incorrect_description_file;
mod main_field;
mod memory_fs;
mod missing;
#[cfg(not(target_os = "windows"))]
mod node_path;
mod package_json;
#[cfg(feature = "yarn_pnp")]
mod pnp;
mod resolve;
mod restrictions;
mod roots;
mod scoped_packages;
mod simple;
mod symlink;
mod tsconfig_paths;
mod tsconfig_project_references;
#[cfg(windows)]
mod windows;

use std::{env, sync::Arc, thread};

use crate::Resolver;

pub fn fixture_root() -> String {
  env::current_dir()
    .unwrap()
    .join("fixtures")
    .to_str()
    .unwrap()
    .to_string()
}

pub fn fixture() -> String {
  let mut p = fixture_root();
  p.push_str("/enhanced_resolve/test/fixtures");
  p
}

/// Test helper: join a `&str` path with a subpath using OS separator.
///
/// Lets tests keep the ergonomic `f.join("a/b")` style while operating on `String`.
/// Normalizes `./` and `..` in the result (so leading `./` is dropped — matching the
/// resolver's internal normalization), but preserves any trailing separator on `sub`
/// the way `Path` component-equality used to swallow.
pub fn p(base: &str, sub: &str) -> String {
  use crate::path::PathUtil;
  let trailing = sub.ends_with('/') || sub.ends_with('\\');
  let mut result = base.normalize_with(sub);
  if trailing && !(result.ends_with('/') || result.ends_with('\\')) {
    result.push('/');
  }
  result
}

/// Path-like join extension for `String`/`&str` so test code can write `f.path_join("a")`.
///
/// Mirrors `Path::join` semantics: absolute `sub` replaces `base`; otherwise components
/// are appended with the platform separator.
pub trait JoinExt {
  fn path_join(&self, sub: &str) -> String;
}

impl JoinExt for str {
  fn path_join(&self, sub: &str) -> String {
    p(self, sub)
  }
}

impl JoinExt for String {
  fn path_join(&self, sub: &str) -> String {
    p(self.as_str(), sub)
  }
}

impl JoinExt for std::path::Path {
  fn path_join(&self, sub: &str) -> String {
    p(self.to_str().expect("path should be UTF-8"), sub)
  }
}

impl JoinExt for std::path::PathBuf {
  fn path_join(&self, sub: &str) -> String {
    p(self.to_str().expect("path should be UTF-8"), sub)
  }
}

#[tokio::test]
async fn threaded_environment() {
  let cwd = env::current_dir().unwrap().to_str().unwrap().to_string();
  let resolver = Arc::new(Resolver::default());
  for _ in 0..2 {
    _ = thread::spawn({
      let cwd = cwd.clone();
      let resolver = Arc::clone(&resolver);
      move || {
        _ = resolver.resolve(cwd, ".");
      }
    })
    .join();
  }
}
