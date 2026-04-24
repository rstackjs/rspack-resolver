//! Tests for tsconfig project references

use crate::{
  ResolveContext, ResolveError, ResolveOptions, Resolver, TsconfigOptions, TsconfigReferences,
};

#[tokio::test]
async fn auto() {
  let f = super::fixture_root().join("tsconfig/cases/project_references");

  let resolver = Resolver::new(ResolveOptions {
    tsconfig: Some(TsconfigOptions {
      config_file: f.join("app"),
      references: TsconfigReferences::Auto,
    }),
    ..ResolveOptions::default()
  });

  #[rustfmt::skip]
    let pass = [
        // Test normal paths alias
        (f.join("app"), "@/index.ts", f.join("app/aliased/index.ts")),
        (f.join("app"), "@/../index.ts", f.join("app/index.ts")),
        // Test project reference
        (f.join("project_a"), "@/index.ts", f.join("project_a/aliased/index.ts")),
        (f.join("project_b/src"), "@/index.ts", f.join("project_b/src/aliased/index.ts")),
        // Does not have paths alias
        (f.join("project_a"), "./index.ts", f.join("project_a/index.ts")),
        (f.join("project_c"), "./index.ts", f.join("project_c/index.ts")),
        // Template variable
        {
            let dir = f.parent().unwrap().join("paths_template_variable");
            (dir.clone(), "foo", dir.join("foo.js"))
        }
    ];

  for (path, request, expected) in pass {
    let resolved_path = resolver
      .resolve(&path, request)
      .await
      .map(|f| f.full_path());
    assert_eq!(resolved_path, Ok(expected), "{request} {path:?}");
  }
}

#[tokio::test]
async fn tscconfig_file_as_file_dependencies() {
  let f = super::fixture_root().join("tsconfig/cases/project_references");

  let resolver = Resolver::new(ResolveOptions {
    tsconfig: Some(TsconfigOptions {
      config_file: f.join("app"),
      references: TsconfigReferences::Auto,
    }),
    ..ResolveOptions::default()
  });
  let mut ctx = ResolveContext::default();

  let resolved_path = resolver
    .resolve_with_context(&f.join("project_b/src"), "@/index.ts", &mut ctx)
    .await
    .map(|f| f.full_path());
  assert_eq!(resolved_path, Ok(f.join("project_b/src/aliased/index.ts")));

  let expected_dependencies = [
    f.join("app/tsconfig.json"),
    f.join("tsconfig.base.json"),
    f.join("project_a/conf.json"),
    f.join("project_b/tsconfig.json"),
    f.join("project_c/tsconfig.json"),
    f.parent()
      .unwrap()
      .join("paths_template_variable/tsconfig2.json"),
  ];
  for dependency in expected_dependencies {
    assert!(
      ctx.file_dependencies.contains(&dependency),
      "missing tsconfig file dependency {dependency:?}: {:?}",
      ctx.file_dependencies
    );
  }
}

#[tokio::test]
async fn disabled() {
  let f = super::fixture_root().join("tsconfig/cases/project_references");

  let resolver = Resolver::new(ResolveOptions {
    tsconfig: Some(TsconfigOptions {
      config_file: f.join("app"),
      references: TsconfigReferences::Disabled,
    }),
    ..ResolveOptions::default()
  });

  #[rustfmt::skip]
    let pass = [
        // Test normal paths alias
        (f.join("app"), "@/index.ts", Ok(f.join("app/aliased/index.ts"))),
        (f.join("app"), "@/../index.ts", Ok(f.join("app/index.ts"))),
        // Test project reference
        (f.join("project_a"), "@/index.ts", Ok(f.join("app/aliased/index.ts"))),
        (f.join("project_b/src"), "@/index.ts", Ok(f.join("app/aliased/index.ts"))),
        // Does not have paths alias
        (f.join("project_a"), "./index.ts", Ok(f.join("project_a/index.ts"))),
        (f.join("project_c"), "./index.ts", Ok(f.join("project_c/index.ts"))),
    ];

  for (path, request, expected) in pass {
    let resolved_path = resolver
      .resolve(&path, request)
      .await
      .map(|f| f.full_path());
    assert_eq!(resolved_path, expected, "{request} {path:?}");
  }
}

#[tokio::test]
async fn manual() {
  let f = super::fixture_root().join("tsconfig/cases/project_references");

  let resolver = Resolver::new(ResolveOptions {
    tsconfig: Some(TsconfigOptions {
      config_file: f.join("app"),
      references: TsconfigReferences::Paths(vec!["../project_a/conf.json".into()]),
    }),
    ..ResolveOptions::default()
  });

  #[rustfmt::skip]
    let pass = [
        // Test normal paths alias
        (f.join("app"), "@/index.ts", Ok(f.join("app/aliased/index.ts"))),
        (f.join("app"), "@/../index.ts", Ok(f.join("app/index.ts"))),
        // Test project reference
        (f.join("project_a"), "@/index.ts", Ok(f.join("project_a/aliased/index.ts"))),
        (f.join("project_b/src"), "@/index.ts", Ok(f.join("app/aliased/index.ts"))),
        // Does not have paths alias
        (f.join("project_a"), "./index.ts", Ok(f.join("project_a/index.ts"))),
        (f.join("project_c"), "./index.ts", Ok(f.join("project_c/index.ts"))),
    ];

  for (path, request, expected) in pass {
    let resolved_path = resolver
      .resolve(&path, request)
      .await
      .map(|f| f.full_path());
    assert_eq!(resolved_path, expected, "{request} {path:?}");
  }
}

#[tokio::test]
async fn self_reference() {
  let f = super::fixture_root().join("tsconfig/cases/project_references");

  #[rustfmt::skip]
    let pass = [
        (f.join("app"), vec!["./tsconfig.json".into()]),
        (f.join("app/tsconfig.json"), vec!["./tsconfig.json".into()]),
        (f.join("app"), vec![f.join("app")]),
        (f.join("app/tsconfig.json"), vec![f.join("app")]),
        (f.join("app/tsconfig.json"), vec![f.join("project_b"), f.join("app")]),
    ];

  for (config_file, reference_paths) in pass {
    let resolver = Resolver::new(ResolveOptions {
      tsconfig: Some(TsconfigOptions {
        config_file: config_file.clone(),
        references: TsconfigReferences::Paths(reference_paths.clone()),
      }),
      ..ResolveOptions::default()
    });
    let path = f.join("app");
    let resolved_path = resolver
      .resolve(&path, "@/index.ts")
      .await
      .map(|f| f.full_path());
    assert_eq!(
      resolved_path,
      Err(ResolveError::TsconfigSelfReference(
        f.join("app/tsconfig.json")
      )),
      "{config_file:?} {reference_paths:?}"
    );
  }
}
