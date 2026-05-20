#[cfg(test)]
mod tests {
  use std::collections::HashSet;

  use crate::{
    path::PathUtil,
    tests::{fixture, JoinExt},
    FileSystemOs, ResolverGeneric,
  };

  #[tokio::test]
  async fn facts_path_compare_use_component_only() {
    // So we assert the equality with path's string other than path itself.
    let path_win = std::path::Path::new(r"d:\test\index.js");
    let path_posix = std::path::Path::new("d:/test/index.js");

    assert_eq!(path_posix, path_win)
  }
  #[tokio::test]
  async fn require_absolution_path_in_windows() {
    let resolver = ResolverGeneric::<FileSystemOs>::new(Default::default());

    let file = fixture().path_join("foo/index.js");
    let pkg_json = fixture().path_join("foo/package.json");
    let file_path_string = file.normalize();
    let pkg_json_path_string = pkg_json.normalize();

    let expected_file_deps = {
      let mut s = HashSet::new();
      s.insert(file_path_string.clone());
      s.insert(pkg_json_path_string.clone());
      s
    };

    // make a posix style  path string e.g  d:/foo/bar.js
    let specifier = file.replace('\\', "/");

    let mut ctx = Default::default();
    let resolved = resolver
      .resolve_with_context(&file, &specifier, &mut ctx)
      .await
      .unwrap();
    let resolved_path_string = resolved.path.to_string();
    let actual_file_deps = ctx
      .file_dependencies
      .iter()
      .map(|p| p.normalize())
      .collect::<HashSet<_>>();

    assert_eq!(resolved_path_string, file_path_string);
    assert_eq!(expected_file_deps, actual_file_deps);
  }
}
