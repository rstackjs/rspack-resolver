use super::memory_fs::MemoryFS;
use crate::{ResolveOptions, ResolverGeneric};

fn new_resolver(
  node_path_dirs: Vec<String>,
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
      vec!["/global/lib".to_string()],
      &[
        ("/global/lib/foo/package.json", r#"{"main":"index.js"}"#),
        ("/global/lib/foo/index.js", ""),
      ],
    );
    let result = resolver.resolve("/project/src", "foo").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok("/global/lib/foo/index.js".to_string())
    );
  }

  #[tokio::test]
  async fn node_modules_takes_priority_over_node_path() {
    let resolver = new_resolver(
      vec!["/global/lib".to_string()],
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
    let result = resolver.resolve("/project/src", "foo").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok("/project/node_modules/foo/index.js".to_string()),
    );
  }

  #[tokio::test]
  async fn multiple_node_path_dirs() {
    let resolver = new_resolver(
      vec!["/first/lib".to_string(), "/second/lib".to_string()],
      &[
        ("/first/lib/foo/package.json", r#"{"main":"index.js"}"#),
        ("/first/lib/foo/index.js", ""),
        ("/second/lib/bar/package.json", r#"{"main":"index.js"}"#),
        ("/second/lib/bar/index.js", ""),
      ],
    );
    let result = resolver.resolve("/project", "foo").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok("/first/lib/foo/index.js".to_string())
    );

    let result = resolver.resolve("/project", "bar").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok("/second/lib/bar/index.js".to_string())
    );
  }

  #[tokio::test]
  async fn first_node_path_dir_takes_priority() {
    let resolver = new_resolver(
      vec!["/first/lib".to_string(), "/second/lib".to_string()],
      &[
        ("/first/lib/foo/package.json", r#"{"main":"a.js"}"#),
        ("/first/lib/foo/a.js", ""),
        ("/second/lib/foo/package.json", r#"{"main":"b.js"}"#),
        ("/second/lib/foo/b.js", ""),
      ],
    );
    let result = resolver.resolve("/project", "foo").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok("/first/lib/foo/a.js".to_string())
    );
  }

  #[tokio::test]
  async fn nonexistent_node_path_dir_skipped() {
    let resolver = new_resolver(
      vec!["/nonexistent".to_string(), "/global/lib".to_string()],
      &[
        ("/global/lib/foo/package.json", r#"{"main":"index.js"}"#),
        ("/global/lib/foo/index.js", ""),
      ],
    );
    let result = resolver.resolve("/project", "foo").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok("/global/lib/foo/index.js".to_string())
    );
  }

  #[tokio::test]
  async fn scoped_package() {
    let resolver = new_resolver(
      vec!["/global/lib".to_string()],
      &[
        (
          "/global/lib/@scope/pkg/package.json",
          r#"{"main":"index.js"}"#,
        ),
        ("/global/lib/@scope/pkg/index.js", ""),
      ],
    );
    let result = resolver.resolve("/project", "@scope/pkg").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok("/global/lib/@scope/pkg/index.js".to_string())
    );
  }

  #[tokio::test]
  async fn subpath_import() {
    let resolver = new_resolver(
      vec!["/global/lib".to_string()],
      &[
        ("/global/lib/foo/package.json", r#"{"main":"index.js"}"#),
        ("/global/lib/foo/index.js", ""),
        ("/global/lib/foo/lib/utils.js", ""),
      ],
    );
    let result = resolver.resolve("/project", "foo/lib/utils").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok("/global/lib/foo/lib/utils.js".to_string())
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
    let result = resolver.resolve("/project", "foo").await;
    assert!(result.is_err());
  }

  #[tokio::test]
  async fn not_found_in_node_path() {
    let resolver = new_resolver(
      vec!["/global/lib".to_string()],
      &[
        ("/global/lib/bar/package.json", r#"{"main":"index.js"}"#),
        ("/global/lib/bar/index.js", ""),
      ],
    );
    let result = resolver.resolve("/project", "foo").await;
    assert!(result.is_err());
  }

  #[tokio::test]
  async fn file_without_package_json() {
    let resolver = new_resolver(
      vec!["/global/lib".to_string()],
      &[("/global/lib/foo.js", "")],
    );
    let result = resolver.resolve("/project", "foo").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok("/global/lib/foo.js".to_string())
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
    .with_node_path_dirs(vec!["/global/lib".to_string()]);
    let result = resolver.resolve("/project", "foo").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok("/global/lib/foo.json".to_string())
    );
  }

  #[tokio::test]
  async fn index_file_resolution() {
    let resolver = new_resolver(
      vec!["/global/lib".to_string()],
      &[("/global/lib/foo/index.js", "")],
    );
    let result = resolver.resolve("/project", "foo").await;
    assert_eq!(
      result.map(|r| r.full_path()),
      Ok("/global/lib/foo/index.js".to_string())
    );
  }
}
