use std::io;

use crate::{FileMetadata, FileSystem};

#[derive(Default)]
pub struct MemoryFS {
  fs: vfs::MemoryFS,
}

impl MemoryFS {
  /// # Panics
  ///
  /// * Fails to create directory
  /// * Fails to write file
  #[allow(dead_code)]
  pub fn new(data: &[(&'static str, &'static str)]) -> Self {
    let mut fs = Self {
      fs: vfs::MemoryFS::default(),
    };
    for (path, content) in data {
      fs.add_file(path, content);
    }
    fs
  }

  #[allow(dead_code)]
  pub fn add_file(&mut self, path: &str, content: &str) {
    use std::path::Path;

    use vfs::FileSystem;
    let fs = &mut self.fs;
    // Create all parent directories
    for ancestor in Path::new(path).ancestors().collect::<Vec<_>>().iter().rev() {
      let ancestor = ancestor.to_str().expect("path should be UTF-8");
      if !fs.exists(ancestor).unwrap() {
        fs.create_dir(ancestor).unwrap();
      }
    }
    // Create file
    let mut file = fs.create_file(path).unwrap();
    file.write_all(content.as_bytes()).unwrap();
  }
}
#[async_trait::async_trait]
impl FileSystem for MemoryFS {
  async fn read_to_string(&self, path: &str) -> io::Result<String> {
    use vfs::FileSystem;
    let mut file = self
      .fs
      .open_file(path)
      .map_err(|err| io::Error::new(io::ErrorKind::NotFound, err))?;
    let mut buffer = String::new();
    file.read_to_string(&mut buffer).unwrap();
    Ok(buffer)
  }
  async fn read(&self, path: &str) -> io::Result<Vec<u8>> {
    let buf = self.read_to_string(path).await?;
    Ok(buf.into_bytes())
  }

  async fn metadata(&self, path: &str) -> io::Result<FileMetadata> {
    use vfs::FileSystem;
    let metadata = self
      .fs
      .metadata(path)
      .map_err(|err| io::Error::new(io::ErrorKind::NotFound, err))?;
    let is_file = metadata.file_type == vfs::VfsFileType::File;
    let is_dir = metadata.file_type == vfs::VfsFileType::Directory;
    Ok(FileMetadata::new(is_file, is_dir, false))
  }

  async fn symlink_metadata(&self, path: &str) -> io::Result<FileMetadata> {
    self.metadata(path).await
  }

  async fn canonicalize(&self, _path: &str) -> io::Result<String> {
    Err(io::Error::new(io::ErrorKind::NotFound, "not a symlink"))
  }
}
