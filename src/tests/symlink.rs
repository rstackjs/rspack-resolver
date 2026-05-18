use std::{fs, io};

use camino::Utf8Path as Path;

use crate::{ResolveOptions, Resolver};

#[derive(Debug, Clone, Copy)]
enum FileType {
  File,
  Dir,
}

#[allow(unused_variables)]
fn symlink(original: &std::path::Path, link: &Path, file_type: FileType) -> io::Result<()> {
  #[cfg(target_family = "unix")]
  {
    std::os::unix::fs::symlink(original, link.as_std_path())
  }

  #[cfg(target_family = "windows")]
  match file_type {
    FileType::File => std::os::windows::fs::symlink_file(original, link.as_std_path()),
    FileType::Dir => std::os::windows::fs::symlink_dir(original, link.as_std_path()),
  }
}

fn init(dirname: &Path, temp_path: &Path) -> io::Result<()> {
  if temp_path.exists() {
    _ = fs::remove_dir_all(temp_path.as_std_path());
  }
  fs::create_dir(temp_path.as_std_path())?;
  symlink(
    dirname.join("../lib/index.js").as_std_path(),
    &temp_path.join("test"),
    FileType::File,
  )?;
  symlink(
    dirname.join("../lib").as_std_path(),
    &temp_path.join("test2"),
    FileType::Dir,
  )?;
  fs::remove_file(temp_path.join("test").as_std_path())?;
  fs::remove_file(temp_path.join("test2").as_std_path())?;
  fs::remove_dir(temp_path.as_std_path())
}

fn create_symlinks(dirname: &Path, temp_path: &Path) -> io::Result<()> {
  fs::create_dir(temp_path.as_std_path()).unwrap();
  let index = dirname.join("../lib/index.js").canonicalize().unwrap();
  symlink(index.as_path(), &temp_path.join("index.js"), FileType::File)?;
  let lib = dirname.join("../lib").canonicalize().unwrap();
  symlink(lib.as_path(), &temp_path.join("lib"), FileType::Dir)?;
  let parent = dirname.join("..").canonicalize().unwrap();
  symlink(parent.as_path(), &temp_path.join("this"), FileType::Dir)?;
  symlink(
    temp_path.join("this").as_std_path(),
    &temp_path.join("that"),
    FileType::Dir,
  )?;
  symlink(
    Path::new("../../lib/index.js").as_std_path(),
    &temp_path.join("node.relative.js"),
    FileType::File,
  )?;
  symlink(
    Path::new("./node.relative.js").as_std_path(),
    &temp_path.join("node.relative.sym.js"),
    FileType::File,
  )?;
  Ok(())
}

fn cleanup_symlinks(temp_path: &Path) {
  _ = fs::remove_dir_all(temp_path.as_std_path());
}

#[tokio::test]
async fn test() -> io::Result<()> {
  let root = super::fixture_root().join("enhanced_resolve");
  let dirname = root.join("test");
  let temp_path = dirname.join("temp");
  if !temp_path.exists() {
    let is_admin = init(&dirname, &temp_path).is_ok();
    if !is_admin {
      return Ok(());
    }
    if let Err(err) = create_symlinks(&dirname, &temp_path) {
      cleanup_symlinks(&temp_path);
      return Err(err);
    }
  }

  let resolver_without_symlinks = Resolver::new(ResolveOptions {
    symlinks: false,
    ..ResolveOptions::default()
  });
  let resolver_with_symlinks = Resolver::default();

  #[rustfmt::skip]
    let pass = [
        ("with a symlink to a file", temp_path.clone(), "./index.js"),
        ("with a relative symlink to a file", temp_path.clone(), "./node.relative.js"),
        ("with a relative symlink to a symlink to a file", temp_path.clone(), "./node.relative.sym.js"),
        ("with a symlink to a directory 1", temp_path.clone(), "./lib/index.js"),
        ("with a symlink to a directory 2", temp_path.clone(), "./this/lib/index.js"),
        ("with multiple symlinks in the path 1", temp_path.clone(), "./this/test/temp/index.js"),
        ("with multiple symlinks in the path 2", temp_path.clone(), "./this/test/temp/lib/index.js"),
        ("with multiple symlinks in the path 3", temp_path.clone(), "./this/test/temp/this/lib/index.js"),
        ("with a symlink to a directory 2 (chained)", temp_path.clone(), "./that/lib/index.js"),
        ("with multiple symlinks in the path 1 (chained)", temp_path.clone(), "./that/test/temp/index.js"),
        ("with multiple symlinks in the path 2 (chained)", temp_path.clone(), "./that/test/temp/lib/index.js"),
        ("with multiple symlinks in the path 3 (chained)", temp_path.clone(), "./that/test/temp/that/lib/index.js"),
        ("with symlinked directory as context 1", temp_path.join( "lib"), "./index.js"),
        ("with symlinked directory as context 2", temp_path.join( "this"), "./lib/index.js"),
        ("with symlinked directory as context and in path", temp_path.join( "this"), "./test/temp/lib/index.js"),
        ("with symlinked directory in context path", temp_path.join( "this/lib"), "./index.js"),
        ("with symlinked directory in context path and symlinked file", temp_path.join( "this/test"), "./temp/index.js"),
        ("with symlinked directory in context path and symlinked directory", temp_path.join( "this/test"), "./temp/lib/index.js"),
        ("with symlinked directory as context 2 (chained)", temp_path.join( "that"), "./lib/index.js"),
        ("with symlinked directory as context and in path (chained)", temp_path.join( "that"), "./test/temp/lib/index.js"),
        ("with symlinked directory in context path (chained)", temp_path.join( "that/lib"), "./index.js"),
        ("with symlinked directory in context path and symlinked file (chained)", temp_path.join( "that/test"), "./temp/index.js"),
        ("with symlinked directory in context path and symlinked directory (chained)", temp_path.join( "that/test"), "./temp/lib/index.js")
    ];

  for (comment, path, request) in pass {
    let filename = resolver_with_symlinks
      .resolve(&path, request)
      .await
      .map(|r| r.full_path());
    assert_eq!(filename, Ok(root.join("lib/index.js")), "{comment:?}");

    let mut ctx = &mut Default::default();
    let resolved_path = resolver_without_symlinks
      .resolve_with_context(&path, request, &mut ctx)
      .await
      .map(|r| r.full_path());
    assert_eq!(resolved_path, Ok(path.join(request)));
    assert!(
      ctx.file_dependencies.contains(&resolved_path.unwrap()),
      "file dependencies should contain resolved path {comment:?}"
    );
  }

  Ok(())
}
