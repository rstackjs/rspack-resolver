//! Not part of enhanced_resolve's test suite

use super::JoinExt;
use crate::{ResolveOptions, Resolver};

#[tokio::test]
async fn test() {
  let f = super::fixture().path_join("restrictions");

  let resolver1 = Resolver::new(ResolveOptions {
    main_fields: vec!["style".into()],
    ..ResolveOptions::default()
  });

  let resolution = resolver1.resolve(&f, "pck2").await.map(|r| r.full_path());
  assert_eq!(resolution, Ok(f.path_join("node_modules/pck2/index.css")));

  let resolver2 = resolver1.clone_with_options(ResolveOptions {
    main_fields: vec!["module".into(), "main".into()],
    ..ResolveOptions::default()
  });

  let resolution = resolver2.resolve(&f, "pck2").await.map(|r| r.full_path());
  assert_eq!(resolution, Ok(f.path_join("node_modules/pck2/module.js")));
}

#[tokio::test]
async fn test_fallback() {
  let f = super::fixture_root().path_join("invalid");

  let resolver1 = Resolver::new(ResolveOptions {
    main_fields: vec!["module".into(), "main".into()],
    extension_alias: vec![(".js".into(), vec![".ts".into(), ".js".into()])],
    ..ResolveOptions::default()
  });

  let resolution = resolver1
    .resolve(&f, "main_field_fallback")
    .await
    .map(|r| r.full_path());
  assert_eq!(
    resolution,
    Ok(f.path_join("node_modules/main_field_fallback/exist.js"))
  );
}
