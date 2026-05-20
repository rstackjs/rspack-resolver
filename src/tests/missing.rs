//! https://github.com/webpack/enhanced-resolve/blob/main/test/missing.test.js

use std::path::Path;

use super::JoinExt;
use crate::{path::PathUtil, AliasValue, ResolveContext, ResolveOptions, Resolver};

#[tokio::test]
async fn test() {
  let f = super::fixture();

  let data = [
    (
      "./missing-file",
      vec![
        f.path_join("missing-file"),
        f.path_join("missing-file.js"),
        f.path_join("missing-file.node"),
      ],
    ),
    (
      "missing-module",
      vec![
        f.path_join("node_modules/missing-module"),
        Path::new(&f).parent().unwrap().path_join("node_modules"), // enhanced-resolve is "node_modules/missing-module"
      ],
    ),
    (
      "missing-module/missing-file",
      vec![
        f.path_join("node_modules/missing-module"),
        // f.parent().unwrap().path_join("node_modules/missing-module"), // we don't report this
      ],
    ),
    (
      "m1/missing-file",
      vec![
        f.path_join("node_modules/m1/missing-file"),
        f.path_join("node_modules/m1/missing-file.js"),
        f.path_join("node_modules/m1/missing-file.node"),
        // f.parent().unwrap().path_join("node_modules/m1"), // we don't report this
      ],
    ),
    (
      "m1/",
      vec![
        f.path_join("node_modules/m1/index"),
        f.path_join("node_modules/m1/index.js"),
        f.path_join("node_modules/m1/index.json"),
        f.path_join("node_modules/m1/index.node"),
      ],
    ),
    ("m1/a", vec![f.path_join("node_modules/m1/a")]),
  ];

  let resolver = Resolver::default();

  for (specifier, missing_dependencies) in data {
    let mut ctx = ResolveContext::default();
    let _ = resolver.resolve_with_context(&f, specifier, &mut ctx).await;

    for path in &ctx.file_dependencies {
      assert_eq!(*path, path.normalize(), "{path:?}");
    }

    for path in missing_dependencies {
      assert_eq!(path, path.normalize(), "{path:?}");
      assert!(
        ctx.missing_dependencies.contains(&path),
        "{specifier}: {path:?} not in {:?}",
        &ctx.missing_dependencies
      );
    }
  }
}

#[tokio::test]
async fn alias_and_extensions() {
  let f = super::fixture();

  let resolver = Resolver::new(ResolveOptions {
    alias: vec![
      (
        "@scope-js/package-name/dir$".into(),
        vec![AliasValue::Path(f.path_join("foo/index.js"))],
      ),
      (
        "react-dom".into(),
        vec![AliasValue::Path(f.path_join("foo/index.js"))],
      ),
    ],
    extensions: vec![".server.ts".into()],

    ..ResolveOptions::default()
  });

  let mut ctx = ResolveContext::default();
  let _ = resolver.resolve_with_context(&f, "@scope-js/package-name/dir/router", &mut ctx);
  let _ = resolver.resolve_with_context(&f, "react-dom/client", &mut ctx);

  for path in &ctx.file_dependencies {
    assert_eq!(*path, path.normalize(), "{path:?}");
  }

  for path in &ctx.missing_dependencies {
    assert_eq!(*path, path.normalize(), "{path:?}");
    if let Some(parent) = Path::new(path).parent() {
      assert!(!parent.is_file(), "{parent:?} must not be a file");
    }
  }
}
