use std::{fs, io, path::Path};

#[cfg(not(target_arch = "wasm32"))]
use cfg_if::cfg_if;
#[cfg(feature = "yarn_pnp")]
use pnp::fs::LruZipCache;
#[cfg(all(feature = "yarn_pnp", not(target_arch = "wasm32")))]
use pnp::fs::{VPath, VPathInfo, ZipCache};

/// File System abstraction used for `ResolverGeneric`
#[async_trait::async_trait]
pub trait FileSystem {
  /// See [std::fs::read]
  ///
  /// # Errors
  ///
  /// See [std::fs::read]
  async fn read(&self, path: &str) -> io::Result<Vec<u8>>;
  /// See [std::fs::read_to_string]
  ///
  /// # Errors
  ///
  /// * See [std::fs::read_to_string]
  /// ## Warning
  /// Use `&str` instead of a generic `P: AsRef<str>` here,
  /// because object safety requirements, it is especially useful, when
  /// you want to store multiple `dyn FileSystem` in a `Vec` or use a `ResolverGeneric<Fs>` in
  /// napi env.
  async fn read_to_string(&self, path: &str) -> io::Result<String>;

  /// See [std::fs::metadata]
  ///
  /// # Errors
  /// See [std::fs::metadata]
  /// ## Warning
  /// Use `&str` instead of a generic `P: AsRef<str>` here,
  /// because object safety requirements, it is especially useful, when
  /// you want to store multiple `dyn FileSystem` in a `Vec` or use a `ResolverGeneric<Fs>` in
  /// napi env.
  async fn metadata(&self, path: &str) -> io::Result<FileMetadata>;

  /// See [std::fs::symlink_metadata]
  ///
  /// # Errors
  ///
  /// See [std::fs::symlink_metadata]
  /// ## Warning
  /// Use `&str` instead of a generic `P: AsRef<str>` here,
  /// because object safety requirements, it is especially useful, when
  /// you want to store multiple `dyn FileSystem` in a `Vec` or use a `ResolverGeneric<Fs>` in
  /// napi env.
  async fn symlink_metadata(&self, path: &str) -> io::Result<FileMetadata>;

  /// See [std::fs::canonicalize]
  ///
  /// # Errors
  ///
  /// See [std::fs::read_link]
  /// ## Warning
  /// Use `&str` instead of a generic `P: AsRef<str>` here,
  /// because object safety requirements, it is especially useful, when
  /// you want to store multiple `dyn FileSystem` in a `Vec` or use a `ResolverGeneric<Fs>` in
  /// napi env.
  async fn canonicalize(&self, path: &str) -> io::Result<String>;
}

/// Metadata information about a file
#[derive(Debug, Clone, Copy)]
pub struct FileMetadata {
  pub is_file: bool,
  pub is_dir: bool,
  pub is_symlink: bool,
}

impl FileMetadata {
  pub fn new(is_file: bool, is_dir: bool, is_symlink: bool) -> Self {
    Self {
      is_file,
      is_dir,
      is_symlink,
    }
  }
}

#[cfg(feature = "yarn_pnp")]
impl From<pnp::fs::FileType> for FileMetadata {
  fn from(value: pnp::fs::FileType) -> Self {
    Self::new(
      value == pnp::fs::FileType::File,
      value == pnp::fs::FileType::Directory,
      false,
    )
  }
}

impl From<fs::Metadata> for FileMetadata {
  fn from(metadata: fs::Metadata) -> Self {
    Self::new(metadata.is_file(), metadata.is_dir(), metadata.is_symlink())
  }
}

pub struct FileSystemOptions {
  #[cfg(feature = "yarn_pnp")]
  pub enable_pnp: bool,
}

impl Default for FileSystemOptions {
  fn default() -> Self {
    Self {
      #[cfg(feature = "yarn_pnp")]
      enable_pnp: true,
    }
  }
}

/// Operating System
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub struct FileSystemOs {
  options: FileSystemOptions,
  #[cfg(feature = "yarn_pnp")]
  pnp_lru: LruZipCache<Vec<u8>>,
}

impl Default for FileSystemOs {
  fn default() -> Self {
    Self {
      options: FileSystemOptions::default(),
      #[cfg(feature = "yarn_pnp")]
      pnp_lru: LruZipCache::new(50, pnp::fs::open_zip_via_read_p),
    }
  }
}

impl FileSystemOs {
  pub fn new(options: FileSystemOptions) -> Self {
    Self {
      options,
      #[cfg(feature = "yarn_pnp")]
      pnp_lru: LruZipCache::new(50, pnp::fs::open_zip_via_read_p),
    }
  }
}

/// UTF-8 wrapper around [`dunce::canonicalize`].
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn canonicalize_to_string<P: AsRef<Path>>(path: P) -> io::Result<String> {
  let canonical = dunce::canonicalize(path)?;
  canonical.into_os_string().into_string().map_err(|_| {
    io::Error::new(
      io::ErrorKind::InvalidData,
      "canonicalized path is not UTF-8",
    )
  })
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait::async_trait]
impl FileSystem for FileSystemOs {
  async fn read(&self, path: &str) -> io::Result<Vec<u8>> {
    let path = Path::new(path);
    cfg_if! {
      if #[cfg(feature = "yarn_pnp")] {
        if self.options.enable_pnp {
            return match VPath::from(path)? {
                VPath::Zip(info) => self.pnp_lru.read(info.physical_base_path(), info.zip_path),
                VPath::Virtual(info) => tokio::fs::read(info.physical_base_path()).await,
                VPath::Native(path) => tokio::fs::read(&path).await,
            }
        }
    }}

    tokio::fs::read(path).await
  }

  async fn read_to_string(&self, path: &str) -> io::Result<String> {
    let path = Path::new(path);
    cfg_if! {
    if #[cfg(feature = "yarn_pnp")] {
        if self.options.enable_pnp {
            return match VPath::from(path)? {
                VPath::Zip(info) => self.pnp_lru.read_to_string(info.physical_base_path(), info.zip_path),
                VPath::Virtual(info) => tokio::fs::read_to_string(info.physical_base_path()).await,
                VPath::Native(path) => tokio::fs::read_to_string(&path).await,
                }
            }
        }
    }
    tokio::fs::read_to_string(path).await
  }

  async fn metadata(&self, path: &str) -> io::Result<FileMetadata> {
    let path = Path::new(path);
    cfg_if! {
        if #[cfg(feature = "yarn_pnp")] {
            if self.options.enable_pnp {
                return match VPath::from(path)? {
                    VPath::Zip(info) => self
                        .pnp_lru
                        .file_type(info.physical_base_path(), info.zip_path)
                        .map(FileMetadata::from),
                    VPath::Virtual(info) => {
                        tokio::fs::metadata(info.physical_base_path())
                            .await
                            .map(FileMetadata::from)
                    }
                    VPath::Native(path) => tokio::fs::metadata(path).await.map(FileMetadata::from),
                }
            }
        }
    }

    tokio::fs::metadata(path).await.map(FileMetadata::from)
  }

  async fn symlink_metadata(&self, path: &str) -> io::Result<FileMetadata> {
    tokio::fs::symlink_metadata(Path::new(path))
      .await
      .map(FileMetadata::from)
  }

  async fn canonicalize(&self, path: &str) -> io::Result<String> {
    let path = Path::new(path);
    cfg_if! {
        if #[cfg(feature = "yarn_pnp")] {
            if self.options.enable_pnp {
                return match VPath::from(path)? {
                    VPath::Zip(info) => {
                        canonicalize_to_string(info.physical_base_path().join(info.zip_path))
                    }
                    VPath::Virtual(info) => canonicalize_to_string(info.physical_base_path()),
                    VPath::Native(path) => canonicalize_to_string(path),
                }
            }
        }
    }

    canonicalize_to_string(path)
  }
}

#[cfg(target_arch = "wasm32")]
#[async_trait::async_trait]
impl FileSystem for FileSystemOs {
  async fn read(&self, path: &str) -> io::Result<Vec<u8>> {
    std::fs::read(Path::new(path))
  }

  async fn read_to_string(&self, path: &str) -> io::Result<String> {
    std::fs::read_to_string(Path::new(path))
  }

  async fn metadata(&self, path: &str) -> io::Result<FileMetadata> {
    let path = Path::new(path);
    // This implementation is verbose because there might be something wrong node wasm runtime.
    // I will investigate it in the future.
    if let Ok(m) = std::fs::metadata(path).map(FileMetadata::from) {
      return Ok(m);
    }

    self
      .symlink_metadata(path.to_str().expect("path should be UTF-8"))
      .await?;
    let resolved = self
      .canonicalize(path.to_str().expect("path should be UTF-8"))
      .await?;
    std::fs::metadata(Path::new(&resolved)).map(FileMetadata::from)
  }

  async fn symlink_metadata(&self, path: &str) -> io::Result<FileMetadata> {
    std::fs::symlink_metadata(Path::new(path)).map(FileMetadata::from)
  }

  async fn canonicalize(&self, path: &str) -> io::Result<String> {
    use std::path::{Component, PathBuf};
    let mut path_buf = PathBuf::from(path);
    let link = fs::read_link(&path_buf)?;
    path_buf.pop();
    for component in link.components() {
      match component {
        Component::ParentDir => {
          path_buf.pop();
        }
        Component::Normal(seg) => {
          path_buf.push(
            seg
              .to_str()
              .expect("path should be UTF-8")
              .trim_end_matches('\0'),
          );
        }
        Component::RootDir => {
          path_buf = PathBuf::from("/");
        }
        Component::CurDir | Component::Prefix(_) => {}
      }

      // This is not performant, we may optimize it with cache in the future
      if fs::symlink_metadata(&path_buf).is_ok_and(|m| m.is_symlink()) {
        let dir = self
          .canonicalize(path_buf.to_str().expect("path should be UTF-8"))
          .await?;
        path_buf = PathBuf::from(dir);
      }
    }
    path_buf.into_os_string().into_string().map_err(|_| {
      io::Error::new(
        io::ErrorKind::InvalidData,
        "canonicalized path is not UTF-8",
      )
    })
  }
}

#[tokio::test]
async fn metadata() {
  let meta = FileMetadata {
    is_file: true,
    is_dir: true,
    is_symlink: true,
  };
  assert_eq!(
    format!("{meta:?}"),
    "FileMetadata { is_file: true, is_dir: true, is_symlink: true }"
  );
  let _ = meta;
}
