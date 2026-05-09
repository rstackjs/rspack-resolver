use std::{
  hash::{Hash, Hasher},
  path::Path,
};

use rustc_hash::FxHasher;

#[derive(Default)]
pub struct IdentityHasher(u64);

impl Hasher for IdentityHasher {
  fn write(&mut self, _: &[u8]) {
    unreachable!("Invalid use of IdentityHasher")
  }

  fn write_u64(&mut self, n: u64) {
    self.0 = n;
  }

  fn finish(&self) -> u64 {
    self.0
  }
}

#[inline]
pub(crate) fn hash_path(path: &Path) -> u64 {
  let mut hasher = FxHasher::default();
  path.hash(&mut hasher);
  hasher.finish()
}
