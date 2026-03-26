use std::path::{Path, PathBuf};

use super::memory_fs::MemoryFS;
use crate::{ResolveOptions, ResolverGeneric};

fn new_resolver(
  node_path_dirs: Vec<PathBuf>,
  data: &[(&'static str, &'static str)],
) -> ResolverGeneric<MemoryFS> {
  ResolverGeneric::<MemoryFS>::new_with_file_system(
    MemoryFS::new(data),
    ResolveOptions {
      node_path: true,
      ..ResolveOptions::default()
    },
  )
  .with_node_path_dirs(node_path_dirs)
}

#[cfg(not(target_os = "windows"))]
mod posix {
  use super::*;

  #[tokio::test]
  async fn basic() {
    let resolver = new_resolver(
      vec![PathBuf::from("/global/lib")],
      &[
        ("/global/lib/foo/package.json", r#"{"main":"index.js"}"#),
        ("/global/lib/foo/index.js", ""),
      ],
    );
    let result = resolver.resolve(Path::new("/project/src"), "foo").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok(PathBuf::from("/global/lib/foo/index.js"))
    );
  }

  #[tokio::test]
  async fn node_modules_takes_priority_over_node_path() {
    let resolver = new_resolver(
      vec![PathBuf::from("/global/lib")],
      &[
        (
          "/project/node_modules/foo/package.json",
          r#"{"main":"index.js"}"#,
        ),
        ("/project/node_modules/foo/index.js", ""),
        ("/global/lib/foo/package.json", r#"{"main":"index.js"}"#),
        ("/global/lib/foo/index.js", ""),
      ],
    );
    let result = resolver.resolve(Path::new("/project/src"), "foo").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok(PathBuf::from("/project/node_modules/foo/index.js")),
    );
  }

  #[tokio::test]
  async fn multiple_node_path_dirs() {
    let resolver = new_resolver(
      vec![PathBuf::from("/first/lib"), PathBuf::from("/second/lib")],
      &[
        ("/first/lib/foo/package.json", r#"{"main":"index.js"}"#),
        ("/first/lib/foo/index.js", ""),
        ("/second/lib/bar/package.json", r#"{"main":"index.js"}"#),
        ("/second/lib/bar/index.js", ""),
      ],
    );
    let result = resolver.resolve(Path::new("/project"), "foo").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok(PathBuf::from("/first/lib/foo/index.js"))
    );

    let result = resolver.resolve(Path::new("/project"), "bar").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok(PathBuf::from("/second/lib/bar/index.js"))
    );
  }

  #[tokio::test]
  async fn first_node_path_dir_takes_priority() {
    let resolver = new_resolver(
      vec![PathBuf::from("/first/lib"), PathBuf::from("/second/lib")],
      &[
        ("/first/lib/foo/package.json", r#"{"main":"a.js"}"#),
        ("/first/lib/foo/a.js", ""),
        ("/second/lib/foo/package.json", r#"{"main":"b.js"}"#),
        ("/second/lib/foo/b.js", ""),
      ],
    );
    let result = resolver.resolve(Path::new("/project"), "foo").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok(PathBuf::from("/first/lib/foo/a.js"))
    );
  }

  #[tokio::test]
  async fn nonexistent_node_path_dir_skipped() {
    let resolver = new_resolver(
      vec![PathBuf::from("/nonexistent"), PathBuf::from("/global/lib")],
      &[
        ("/global/lib/foo/package.json", r#"{"main":"index.js"}"#),
        ("/global/lib/foo/index.js", ""),
      ],
    );
    let result = resolver.resolve(Path::new("/project"), "foo").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok(PathBuf::from("/global/lib/foo/index.js"))
    );
  }

  #[tokio::test]
  async fn scoped_package() {
    let resolver = new_resolver(
      vec![PathBuf::from("/global/lib")],
      &[
        (
          "/global/lib/@scope/pkg/package.json",
          r#"{"main":"index.js"}"#,
        ),
        ("/global/lib/@scope/pkg/index.js", ""),
      ],
    );
    let result = resolver.resolve(Path::new("/project"), "@scope/pkg").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok(PathBuf::from("/global/lib/@scope/pkg/index.js"))
    );
  }

  #[tokio::test]
  async fn subpath_import() {
    let resolver = new_resolver(
      vec![PathBuf::from("/global/lib")],
      &[
        ("/global/lib/foo/package.json", r#"{"main":"index.js"}"#),
        ("/global/lib/foo/index.js", ""),
        ("/global/lib/foo/lib/utils.js", ""),
      ],
    );
    let result = resolver
      .resolve(Path::new("/project"), "foo/lib/utils")
      .await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok(PathBuf::from("/global/lib/foo/lib/utils.js"))
    );
  }

  #[tokio::test]
  async fn disabled_node_path() {
    let resolver = ResolverGeneric::<MemoryFS>::new_with_file_system(
      MemoryFS::new(&[
        ("/global/lib/foo/package.json", r#"{"main":"index.js"}"#),
        ("/global/lib/foo/index.js", ""),
      ]),
      ResolveOptions {
        node_path: false,
        ..ResolveOptions::default()
      },
    );
    let result = resolver.resolve(Path::new("/project"), "foo").await;
    assert!(result.is_err());
  }

  #[tokio::test]
  async fn not_found_in_node_path() {
    let resolver = new_resolver(
      vec![PathBuf::from("/global/lib")],
      &[
        ("/global/lib/bar/package.json", r#"{"main":"index.js"}"#),
        ("/global/lib/bar/index.js", ""),
      ],
    );
    let result = resolver.resolve(Path::new("/project"), "foo").await;
    assert!(result.is_err());
  }

  #[tokio::test]
  async fn file_without_package_json() {
    let resolver = new_resolver(
      vec![PathBuf::from("/global/lib")],
      &[("/global/lib/foo.js", "")],
    );
    let result = resolver.resolve(Path::new("/project"), "foo").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok(PathBuf::from("/global/lib/foo.js"))
    );
  }

  #[tokio::test]
  async fn resolve_with_extensions() {
    let resolver = ResolverGeneric::<MemoryFS>::new_with_file_system(
      MemoryFS::new(&[("/global/lib/foo.json", "{}")]),
      ResolveOptions {
        node_path: true,
        extensions: vec![".js".into(), ".json".into()],
        ..ResolveOptions::default()
      },
    )
    .with_node_path_dirs(vec![PathBuf::from("/global/lib")]);
    let result = resolver.resolve(Path::new("/project"), "foo").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok(PathBuf::from("/global/lib/foo.json"))
    );
  }

  #[tokio::test]
  async fn index_file_resolution() {
    let resolver = new_resolver(
      vec![PathBuf::from("/global/lib")],
      &[("/global/lib/foo/index.js", "")],
    );
    let result = resolver.resolve(Path::new("/project"), "foo").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok(PathBuf::from("/global/lib/foo/index.js"))
    );
  }
}

#[cfg(target_os = "windows")]
mod windows {
  use super::*;

  #[tokio::test]
  async fn basic_windows() {
    let resolver = new_resolver(
      vec![PathBuf::from(r"C:\global\lib")],
      &[
        (r"C:\global\lib\foo\package.json", r#"{"main":"index.js"}"#),
        (r"C:\global\lib\foo\index.js", ""),
      ],
    );
    let result = resolver.resolve(Path::new(r"C:\project\src"), "foo").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok(PathBuf::from(r"C:\global\lib\foo\index.js")),
    );
  }

  #[tokio::test]
  async fn node_modules_takes_priority_over_node_path_windows() {
    let resolver = new_resolver(
      vec![PathBuf::from(r"C:\global\lib")],
      &[
        (
          r"C:\project\node_modules\foo\package.json",
          r#"{"main":"index.js"}"#,
        ),
        (r"C:\project\node_modules\foo\index.js", ""),
        (r"C:\global\lib\foo\package.json", r#"{"main":"index.js"}"#),
        (r"C:\global\lib\foo\index.js", ""),
      ],
    );
    let result = resolver.resolve(Path::new(r"C:\project\src"), "foo").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok(PathBuf::from(r"C:\project\node_modules\foo\index.js")),
    );
  }

  #[tokio::test]
  async fn scoped_package_windows() {
    let resolver = new_resolver(
      vec![PathBuf::from(r"C:\global\lib")],
      &[
        (
          r"C:\global\lib\@scope\pkg\package.json",
          r#"{"main":"index.js"}"#,
        ),
        (r"C:\global\lib\@scope\pkg\index.js", ""),
      ],
    );
    let result = resolver
      .resolve(Path::new(r"C:\project"), "@scope/pkg")
      .await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok(PathBuf::from(r"C:\global\lib\@scope\pkg\index.js")),
    );
  }

  #[tokio::test]
  async fn multiple_node_path_dirs_windows() {
    let resolver = new_resolver(
      vec![
        PathBuf::from(r"C:\first\lib"),
        PathBuf::from(r"D:\second\lib"),
      ],
      &[
        (r"C:\first\lib\foo\package.json", r#"{"main":"index.js"}"#),
        (r"C:\first\lib\foo\index.js", ""),
        (r"D:\second\lib\bar\package.json", r#"{"main":"index.js"}"#),
        (r"D:\second\lib\bar\index.js", ""),
      ],
    );
    let result = resolver.resolve(Path::new(r"C:\project"), "foo").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok(PathBuf::from(r"C:\first\lib\foo\index.js"))
    );

    let result = resolver.resolve(Path::new(r"C:\project"), "bar").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok(PathBuf::from(r"D:\second\lib\bar\index.js"))
    );
  }
}

#[cfg(test)]
mod parse_env {
  use crate::ResolveOptions;

  #[test]
  fn parse_node_path_posix_delimiter() {
    let paths: Vec<_> = "/usr/lib/node:/home/user/.node_modules"
      .split(':')
      .filter(|s| !s.is_empty())
      .map(std::path::PathBuf::from)
      .collect();
    assert_eq!(paths.len(), 2);
    assert_eq!(paths[0], std::path::PathBuf::from("/usr/lib/node"));
    assert_eq!(
      paths[1],
      std::path::PathBuf::from("/home/user/.node_modules")
    );
  }

  #[test]
  fn parse_node_path_windows_delimiter() {
    let paths: Vec<_> = r"C:\Users\node;D:\libs\node"
      .split(';')
      .filter(|s| !s.is_empty())
      .map(std::path::PathBuf::from)
      .collect();
    assert_eq!(paths.len(), 2);
    assert_eq!(paths[0], std::path::PathBuf::from(r"C:\Users\node"));
    assert_eq!(paths[1], std::path::PathBuf::from(r"D:\libs\node"));
  }

  #[test]
  fn parse_empty_string() {
    let paths: Vec<_> = ""
      .split(':')
      .filter(|s| !s.is_empty())
      .map(std::path::PathBuf::from)
      .collect();
    assert!(paths.is_empty());
  }

  #[test]
  fn parse_trailing_delimiter() {
    let paths: Vec<_> = "/a:/b:"
      .split(':')
      .filter(|s| !s.is_empty())
      .map(std::path::PathBuf::from)
      .collect();
    assert_eq!(paths.len(), 2);
  }

  #[test]
  fn default_node_path_is_false() {
    let options = ResolveOptions::default();
    assert!(!options.node_path);
  }
}
