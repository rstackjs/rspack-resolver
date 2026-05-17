//! https://github.com/webpack/enhanced-resolve/blob/main/test/dependencies.test.js

#[cfg(not(target_os = "windows"))] // MemoryFS's path separator is always `/` so the test will not pass in windows.
mod windows {
  use std::{
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
  };

  use rustc_hash::{FxHashSet, FxHasher};

  use super::super::memory_fs::MemoryFS;
  use crate::{ResolveContext, ResolveOptions, ResolvePreHashedContext, ResolverGeneric};

  fn path_hash(path: &Path) -> u64 {
    let mut hasher = FxHasher::default();
    path.hash(&mut hasher);
    hasher.finish()
  }

  fn file_system() -> MemoryFS {
    MemoryFS::new(&[
      ("/a/b/node_modules/some-module/index.js", ""),
      (
        "/a/node_modules/module/package.json",
        r#"{"main":"entry.js"}"#,
      ),
      ("/a/node_modules/module/file.js", r#"{"main":"entry.js"}"#),
      ("/modules/other-module/file.js", ""),
    ])
  }

  #[tokio::test]
  async fn test() {
    let file_system = file_system();

    let resolver = ResolverGeneric::<MemoryFS>::new_with_file_system(
      file_system,
      ResolveOptions {
        extensions: vec![".json".into(), ".js".into()],
        modules: vec!["/modules".into(), "node_modules".into()],
        ..ResolveOptions::default()
      },
    );

    let data = [
      (
        "middle module request",
        "/a/b/c",
        "module/file",
        "/a/node_modules/module/file.js",
        // These dependencies are different from enhanced-resolve due to different code path to
        // querying the file system
        vec![
          // found package.json
          "/a/node_modules/module/package.json",
          // symlink checks
          "/a/node_modules/module/file.js",
          // "/a/node_modules/module",
          // "/a/node_modules",
          // "/a",
          // "/",
        ],
        vec![
          // missing package.jsons
          // "/a/b/c/package.json",
          "/a/b/package.json",
          "/a/package.json",
          "/package.json",
          // missing modules directories
          "/a/b/c",
          // "/a/b/c/node_modules",
          // missing single file modules
          "/modules/module",
          "/a/b/node_modules/module",
          // missing files with alternative extensions
          "/a/node_modules/module/file",
          "/a/node_modules/module/file.json",
        ],
      ),
      (
        "fast found module",
        "/a/b/c",
        "other-module/file.js",
        "/modules/other-module/file.js",
        // These dependencies are different from enhanced-resolve due to different code path to
        // querying the file system
        vec![
          // symlink checks
          "/modules/other-module/file.js",
          // "/modules/other-module",
          // "/modules",
          // "/",
        ],
        vec![
          // missing package.jsons
          // "/a/b/c/package.json",
          "/a/b/c",
          "/a/b/package.json",
          "/a/package.json",
          "/package.json",
          "/modules/other-module/package.json",
          "/modules/package.json",
        ],
      ),
    ];

    for (name, context, request, result, file_dependencies, missing_dependencies) in data {
      let mut ctx = ResolveContext::default();
      let path = PathBuf::from(context);
      let resolved_path = resolver
        .resolve_with_context(&path, request, &mut ctx)
        .await
        .map(|r| r.full_path());
      assert_eq!(resolved_path, Ok(PathBuf::from(result)));
      let expected_file_dependencies = file_dependencies
        .iter()
        .map(PathBuf::from)
        .collect::<FxHashSet<_>>();
      let expected_missing_dependencies = missing_dependencies
        .iter()
        .map(PathBuf::from)
        .collect::<FxHashSet<_>>();
      assert_eq!(ctx.file_dependencies, expected_file_dependencies, "{name}");
      assert_eq!(
        ctx.missing_dependencies, expected_missing_dependencies,
        "{name}"
      );

      let mut prehashed_ctx = ResolvePreHashedContext::default();
      let prehashed_resolved = resolver
        .resolve_with_prehashed_context(&path, request, &mut prehashed_ctx)
        .await
        .map(|r| r.full_path());
      assert_eq!(prehashed_resolved, Ok(PathBuf::from(result)));
      assert!(prehashed_ctx
        .file_dependencies
        .iter()
        .all(|dependency| { dependency.precomputed_hash() == path_hash(dependency.path()) }));
      assert!(prehashed_ctx
        .missing_dependencies
        .iter()
        .all(|dependency| { dependency.precomputed_hash() == path_hash(dependency.path()) }));
      let prehashed_file_dependencies = prehashed_ctx
        .file_dependencies
        .iter()
        .map(|d| d.path().to_owned())
        .collect::<FxHashSet<_>>();
      let prehashed_missing_dependencies = prehashed_ctx
        .missing_dependencies
        .iter()
        .map(|d| d.path().to_owned())
        .collect::<FxHashSet<_>>();
      assert_eq!(
        prehashed_file_dependencies, expected_file_dependencies,
        "{name}"
      );
      assert_eq!(
        prehashed_missing_dependencies, expected_missing_dependencies,
        "{name}"
      );
    }
  }
}
