//! UTF-8 path types used inside the resolver.
//!
//! The previous design followed `std::path::Path` exactly — a borrowed
//! `#[repr(transparent)] struct ResolverPath(str)`. We now switch the
//! borrowed form to a **sized** value type
//! `ResolverPath<'a> { prehash: u64, inner: &'a str }` so it can carry a
//! precomputed FxHash of the path bytes, the same way [`ResolverPathBuf`]
//! does for owned paths.
//!
//! Why: the cache hot path keeps re-hashing equivalent paths even when the
//! caller already knows the hash. Embedding the hash in the borrow lets
//! every layer that handles a path forward the value for free, instead of
//! recomputing FxHash bytes-by-bytes at each layer.
//!
//! ## Semantics
//! - On `cfg(unix)` only `/` is a separator.
//! - On `cfg(windows)` both `/` and `\` are separators; drive letters (`C:`),
//!   UNC roots (`\\server\share`), and verbatim (`\\?\…`) prefixes are
//!   recognized as `Component::Prefix`.
//!
//! ## Scope
//! Used inside the crate where it actually cuts hash / string-conversion
//! cost (cache lookups, `Resolution` return values). Other modules that
//! just need quick `parent()` / `file_name()` style operations on a one-off
//! `&str` can still construct a `ResolverPath` ad hoc — the hash is cheap
//! enough that paying it once per construction is the right default.

// This module deliberately mirrors `std::path` naming, so a few clippy lints
// that ordinarily catch over-eager API surface are suppressed.
#![allow(
  clippy::use_self,
  clippy::unnecessary_wraps,
  clippy::elidable_lifetime_names,
  clippy::needless_continue
)]

use std::{
  cmp::Ordering,
  fmt,
  hash::{BuildHasher, BuildHasherDefault, Hash, Hasher},
};

use rustc_hash::FxHasher;

/// FxHash of a path slice, matching the hash other parts of the crate
/// compute via `FxHasher::default()` + `str::hash`. Centralized so the
/// precomputed value carried on [`ResolverPath`] / [`ResolverPathBuf`]
/// agrees across borrowed/owned forms.
#[inline]
pub fn hash_path(s: &str) -> u64 {
  let mut h = BuildHasherDefault::<FxHasher>::default().build_hasher();
  s.hash(&mut h);
  h.finish()
}

// ---------------------------------------------------------------------------
// Separator + platform helpers
// ---------------------------------------------------------------------------

/// Native separator pushed when joining components.
#[cfg(windows)]
pub const MAIN_SEPARATOR: char = '\\';
#[cfg(not(windows))]
pub const MAIN_SEPARATOR: char = '/';

#[cfg(windows)]
#[inline]
const fn is_sep_byte(b: u8) -> bool {
  b == b'/' || b == b'\\'
}
#[cfg(not(windows))]
#[inline]
const fn is_sep_byte(b: u8) -> bool {
  b == b'/'
}

#[inline]
fn is_verbatim_sep_byte(b: u8) -> bool {
  b == b'\\'
}

/// Length of the byte sequence that constitutes the path's *prefix component*
/// (drive letter, UNC, verbatim, …). On non-Windows targets this is always 0.
#[cfg(not(windows))]
fn prefix_len(_path: &str) -> usize {
  0
}

#[cfg(windows)]
fn prefix_len(path: &str) -> usize {
  parse_windows_prefix(path).map_or(0, |p| p.len)
}

#[cfg(windows)]
#[derive(Clone, Copy)]
struct WindowsPrefix {
  len: usize,
  /// Verbatim prefixes (`\\?\…`) disable forward-slash separators.
  verbatim: bool,
}

#[cfg(windows)]
fn parse_windows_prefix(path: &str) -> Option<WindowsPrefix> {
  let bytes = path.as_bytes();
  // `\\?\…` verbatim, `\\.\…` device, or `\\server\share` UNC
  if bytes.len() >= 2 && is_sep_byte(bytes[0]) && is_sep_byte(bytes[1]) {
    // verbatim or device
    if bytes.len() >= 4 && (bytes[2] == b'?' || bytes[2] == b'.') && is_sep_byte(bytes[3]) {
      let verbatim = bytes[2] == b'?';
      // `\\?\UNC\server\share`
      if verbatim
        && bytes.len() >= 8
        && bytes[4..7].eq_ignore_ascii_case(b"UNC")
        && is_sep_byte(bytes[7])
      {
        let after = &bytes[8..];
        let server_end = after
          .iter()
          .position(|&b| is_verbatim_sep_byte(b))
          .unwrap_or(after.len());
        if server_end == 0 {
          return None;
        }
        let share_start = server_end + 1;
        if share_start >= after.len() {
          return None;
        }
        let share_end_rel = after[share_start..]
          .iter()
          .position(|&b| is_verbatim_sep_byte(b))
          .unwrap_or(after.len() - share_start);
        return Some(WindowsPrefix {
          len: 8 + share_start + share_end_rel,
          verbatim: true,
        });
      }
      // `\\?\C:` verbatim disk
      if verbatim && bytes.len() >= 6 && bytes[4].is_ascii_alphabetic() && bytes[5] == b':' {
        return Some(WindowsPrefix {
          len: 6,
          verbatim: true,
        });
      }
      // Generic verbatim `\\?\foo` or device `\\.\foo` — single segment.
      let rest = &bytes[4..];
      let end = rest
        .iter()
        .position(|&b| is_verbatim_sep_byte(b))
        .unwrap_or(rest.len());
      return Some(WindowsPrefix {
        len: 4 + end,
        verbatim,
      });
    }
    // `\\server\share` UNC
    let after = &bytes[2..];
    let server_end = after
      .iter()
      .position(|&b| is_sep_byte(b))
      .unwrap_or(after.len());
    if server_end == 0 {
      return None;
    }
    let share_start = server_end + 1;
    if share_start >= after.len() {
      return None;
    }
    let share_end_rel = after[share_start..]
      .iter()
      .position(|&b| is_sep_byte(b))
      .unwrap_or(after.len() - share_start);
    return Some(WindowsPrefix {
      len: 2 + share_start + share_end_rel,
      verbatim: false,
    });
  }
  // `C:` drive prefix
  if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
    return Some(WindowsPrefix {
      len: 2,
      verbatim: false,
    });
  }
  None
}

// ---------------------------------------------------------------------------
// Component / Components
// ---------------------------------------------------------------------------

/// Iterator yield type — mirrors `std::path::Component`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Component<'a> {
  Prefix(&'a str),
  RootDir,
  CurDir,
  ParentDir,
  Normal(&'a str),
}

impl<'a> Component<'a> {
  pub fn as_str(self) -> &'a str {
    match self {
      Self::Prefix(s) | Self::Normal(s) => s,
      Self::RootDir => "/",
      Self::CurDir => ".",
      Self::ParentDir => "..",
    }
  }
}

#[derive(Clone)]
pub struct Components<'a> {
  rest: &'a str,
  prefix: Option<&'a str>,
  has_root: bool,
  verbatim: bool,
  /// Once the front cursor has yielded anything, a subsequent `.` segment is
  /// dropped (matching `std::path` — `./a/b` keeps a leading CurDir, but
  /// `a/./b` drops the middle one).
  front_started: bool,
}

impl<'a> Components<'a> {
  fn new(path: &'a str) -> Self {
    let plen = prefix_len(path);
    let prefix = if plen > 0 { Some(&path[..plen]) } else { None };
    let verbatim = {
      #[cfg(windows)]
      {
        prefix.is_some() && path.starts_with(r"\\?")
      }
      #[cfg(not(windows))]
      {
        false
      }
    };
    let after_prefix = &path[plen..];
    let (has_root, rest) = match after_prefix.as_bytes().first() {
      Some(&b) if is_sep_byte(b) => (true, &after_prefix[1..]),
      _ => (false, after_prefix),
    };
    Self {
      rest,
      prefix,
      has_root,
      verbatim,
      front_started: false,
    }
  }

  pub fn as_str(&self) -> &'a str {
    self.rest
  }

  fn is_sep(&self, b: u8) -> bool {
    if self.verbatim {
      is_verbatim_sep_byte(b)
    } else {
      is_sep_byte(b)
    }
  }

  /// Total unread byte length (prefix + root + rest) — used to recover the
  /// parent slice in `ResolverPath::parent`.
  fn remaining_len(&self) -> usize {
    let mut n = self.rest.len();
    if self.has_root {
      n += 1;
    }
    if let Some(p) = self.prefix {
      n += p.len();
    }
    n
  }
}

impl<'a> Iterator for Components<'a> {
  type Item = Component<'a>;

  fn next(&mut self) -> Option<Component<'a>> {
    if let Some(p) = self.prefix.take() {
      self.front_started = true;
      return Some(Component::Prefix(p));
    }
    if self.has_root {
      self.has_root = false;
      self.front_started = true;
      return Some(Component::RootDir);
    }
    loop {
      while let Some((&b, tail)) = self.rest.as_bytes().split_first() {
        if self.is_sep(b) {
          // SAFETY: ASCII separator split preserves UTF-8 boundary.
          self.rest = unsafe { std::str::from_utf8_unchecked(tail) };
        } else {
          break;
        }
      }
      if self.rest.is_empty() {
        return None;
      }
      let end = self
        .rest
        .as_bytes()
        .iter()
        .position(|&b| self.is_sep(b))
        .unwrap_or(self.rest.len());
      let segment = &self.rest[..end];
      self.rest = &self.rest[end..];
      match segment {
        "." if self.front_started => continue,
        "." => {
          self.front_started = true;
          return Some(Component::CurDir);
        }
        ".." => {
          self.front_started = true;
          return Some(Component::ParentDir);
        }
        other => {
          self.front_started = true;
          return Some(Component::Normal(other));
        }
      }
    }
  }
}

impl<'a> DoubleEndedIterator for Components<'a> {
  fn next_back(&mut self) -> Option<Component<'a>> {
    loop {
      while let Some((&b, head)) = self.rest.as_bytes().split_last() {
        if self.is_sep(b) {
          // SAFETY: ASCII separator split preserves UTF-8 boundary.
          self.rest = unsafe { std::str::from_utf8_unchecked(head) };
        } else {
          break;
        }
      }
      if !self.rest.is_empty() {
        let start = self
          .rest
          .as_bytes()
          .iter()
          .rposition(|&b| self.is_sep(b))
          .map_or(0, |i| i + 1);
        let segment = &self.rest[start..];
        self.rest = &self.rest[..start];
        match segment {
          "." => {
            if self.rest.is_empty() && self.prefix.is_none() && !self.has_root {
              return Some(Component::CurDir);
            }
            continue;
          }
          ".." => return Some(Component::ParentDir),
          other => return Some(Component::Normal(other)),
        }
      }
      if self.has_root {
        self.has_root = false;
        return Some(Component::RootDir);
      }
      return self.prefix.take().map(Component::Prefix);
    }
  }
}

// ---------------------------------------------------------------------------
// ResolverPath (sized borrow)
// ---------------------------------------------------------------------------

/// Borrowed UTF-8 path with a precomputed FxHash.
///
/// `Copy`, 24 bytes on 64-bit (`u64` hash + `&str` fat pointer). Construct
/// via [`new`][Self::new] for any ad-hoc `&str`, or
/// [`with_prehash`][Self::with_prehash] when the caller already paid the
/// hash cost (e.g. a cache lookup that hashes the key up front).
#[derive(Clone, Copy)]
pub struct ResolverPath<'a> {
  prehash: u64,
  inner: &'a str,
}

impl<'a> ResolverPath<'a> {
  /// Wrap a string slice as a path. Computes the prehash eagerly — the
  /// FxHash of even a long path is a few nanoseconds, and forwarding the
  /// value through subsequent layers avoids repeated work.
  #[inline]
  pub fn new(s: &'a str) -> Self {
    Self {
      prehash: hash_path(s),
      inner: s,
    }
  }

  /// Wrap a string slice with a hash the caller has already computed (e.g.
  /// the cache hashing the key for lookup before deciding to allocate).
  /// `debug_assertions` enforces consistency.
  #[inline]
  pub fn with_prehash(prehash: u64, s: &'a str) -> Self {
    debug_assert_eq!(prehash, hash_path(s), "prehash does not match string");
    Self { prehash, inner: s }
  }

  /// Underlying string slice.
  #[inline]
  pub fn as_str(self) -> &'a str {
    self.inner
  }

  /// Precomputed FxHash. Single field read, no rehashing.
  #[inline]
  pub fn precomputed_hash(self) -> u64 {
    self.prehash
  }

  pub fn is_empty(self) -> bool {
    self.inner.is_empty()
  }

  pub fn is_absolute(self) -> bool {
    let plen = prefix_len(self.inner);
    let after = &self.inner.as_bytes()[plen..];
    #[cfg(windows)]
    {
      if plen == 0 {
        return false;
      }
      if matches!(after.first(), Some(&b) if is_sep_byte(b)) {
        return true;
      }
      let bytes = self.inner.as_bytes();
      bytes.starts_with(b"\\\\") || bytes.starts_with(b"//")
    }
    #[cfg(not(windows))]
    {
      let _ = plen;
      matches!(after.first(), Some(&b) if is_sep_byte(b))
    }
  }

  pub fn is_relative(self) -> bool {
    !self.is_absolute()
  }

  /// Component iterator.
  pub fn components(self) -> Components<'a> {
    Components::new(self.inner)
  }

  /// Parent path. Returns `None` for a path that is just a prefix and/or root.
  pub fn parent(self) -> Option<ResolverPath<'a>> {
    let mut comps = self.components();
    let last = comps.next_back()?;
    match last {
      Component::Normal(_) | Component::CurDir | Component::ParentDir => {
        let mut end = comps.remaining_len();
        let root_keep = prefix_len(self.inner) + usize::from(self.has_root_separator());
        while end > root_keep {
          let b = self.inner.as_bytes()[end - 1];
          if comps.is_sep(b) {
            end -= 1;
          } else {
            break;
          }
        }
        // Parent hash is recomputed: it is a different slice from `self`,
        // and FxHash on a few hundred bytes is cheap.
        Some(ResolverPath::new(&self.inner[..end]))
      }
      Component::Prefix(_) | Component::RootDir => None,
    }
  }

  pub fn file_name(self) -> Option<&'a str> {
    self.components().next_back().and_then(|c| match c {
      Component::Normal(s) => Some(s),
      _ => None,
    })
  }

  pub fn file_stem(self) -> Option<&'a str> {
    let name = self.file_name()?;
    Some(rsplit_file_at_dot(name).0)
  }

  pub fn extension(self) -> Option<&'a str> {
    let name = self.file_name()?;
    rsplit_file_at_dot(name).1
  }

  /// Component-aware prefix check.
  pub fn starts_with<S: AsRef<str>>(self, base: S) -> bool {
    iter_starts_with(self.components(), Components::new(base.as_ref()))
  }

  /// Component-aware suffix check.
  pub fn ends_with<S: AsRef<str>>(self, child: S) -> bool {
    iter_ends_with(self.components(), Components::new(child.as_ref()))
  }

  /// Strip the given component-aware prefix, returning the tail. Recomputes
  /// the tail's hash (different slice).
  pub fn strip_prefix<S: AsRef<str>>(self, base: S) -> Result<ResolverPath<'a>, StripPrefixError> {
    let mut s_comps = self.components();
    let mut b_comps = Components::new(base.as_ref());
    loop {
      match b_comps.next() {
        None => {
          let rest = s_comps.rest;
          let trimmed =
            rest.trim_start_matches(|c: char| c == '\\' || (!s_comps.verbatim && c == '/'));
          return Ok(ResolverPath::new(trimmed));
        }
        Some(b) => match s_comps.next() {
          Some(s) if components_equal(s, b) => continue,
          _ => return Err(StripPrefixError(())),
        },
      }
    }
  }

  pub fn join<S: AsRef<str>>(self, other: S) -> ResolverPathBuf {
    let mut buf = self.to_path_buf();
    buf.push(ResolverPath::new(other.as_ref()));
    buf
  }

  pub fn with_extension(self, new_ext: &str) -> ResolverPathBuf {
    let mut buf = self.to_path_buf();
    buf.set_extension(new_ext);
    buf
  }

  pub fn with_file_name(self, file_name: &str) -> ResolverPathBuf {
    let mut buf = self.to_path_buf();
    buf.set_file_name(file_name);
    buf
  }

  pub fn ancestors(self) -> Ancestors<'a> {
    Ancestors { next: Some(self) }
  }

  /// Allocate an owned copy. The prehash is reused so we don't recompute.
  pub fn to_path_buf(self) -> ResolverPathBuf {
    ResolverPathBuf::with_prehash(self.prehash, self.inner.to_string())
  }

  pub fn display(self) -> &'a str {
    self.inner
  }

  // -- internal helpers --

  fn has_root_separator(self) -> bool {
    let plen = prefix_len(self.inner);
    matches!(self.inner.as_bytes().get(plen), Some(&b) if is_sep_byte(b))
  }
}

// -- Trait impls for ResolverPath ------------------------------------------

impl<'a> AsRef<str> for ResolverPath<'a> {
  fn as_ref(&self) -> &str {
    self.inner
  }
}

impl<'a> fmt::Debug for ResolverPath<'a> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Debug::fmt(self.inner, f)
  }
}

impl<'a> fmt::Display for ResolverPath<'a> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Display::fmt(self.inner, f)
  }
}

impl<'a> PartialEq for ResolverPath<'a> {
  fn eq(&self, other: &Self) -> bool {
    self.components().eq(other.components())
  }
}
impl<'a> Eq for ResolverPath<'a> {}

impl<'a> PartialOrd for ResolverPath<'a> {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}
impl<'a> Ord for ResolverPath<'a> {
  fn cmp(&self, other: &Self) -> Ordering {
    self.components().cmp(other.components())
  }
}

impl<'a> Hash for ResolverPath<'a> {
  /// Writes only the prehash. Compatible with [`ResolverPathBuf`]'s `Hash`
  /// impl, so a `HashMap` keyed on the borrowed form looks up an owned key
  /// for the same path bytes correctly.
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.prehash.hash(state);
  }
}

// ---------------------------------------------------------------------------
// ResolverPathBuf (owned)
// ---------------------------------------------------------------------------

/// Owned UTF-8 path. Sibling of [`ResolverPath`]; both carry a precomputed
/// FxHash, kept in sync by every mutating method.
#[derive(Clone)]
pub struct ResolverPathBuf {
  prehash: u64,
  inner: String,
}

impl Default for ResolverPathBuf {
  fn default() -> Self {
    Self {
      prehash: hash_path(""),
      inner: String::new(),
    }
  }
}

impl ResolverPathBuf {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn from_string(s: String) -> Self {
    let prehash = hash_path(&s);
    Self { prehash, inner: s }
  }

  /// Build from a string whose hash the caller already computed.
  #[inline]
  pub fn with_prehash(prehash: u64, s: String) -> Self {
    debug_assert_eq!(prehash, hash_path(&s), "prehash does not match string");
    Self { prehash, inner: s }
  }

  /// Borrow as the sized [`ResolverPath`] view. Forwards the precomputed
  /// hash without recomputation.
  #[inline]
  pub fn as_path(&self) -> ResolverPath<'_> {
    ResolverPath {
      prehash: self.prehash,
      inner: &self.inner,
    }
  }

  pub fn as_str(&self) -> &str {
    &self.inner
  }

  pub fn into_string(self) -> String {
    self.inner
  }

  /// Precomputed FxHash of the contained path.
  #[inline]
  pub fn precomputed_hash(&self) -> u64 {
    self.prehash
  }

  #[inline]
  fn rehash(&mut self) {
    self.prehash = hash_path(&self.inner);
  }

  // -- mirrored convenience accessors so callers don't have to write
  // -- `.as_path().parent()` etc. --------------------------------------------

  pub fn parent(&self) -> Option<ResolverPath<'_>> {
    self.as_path().parent()
  }

  pub fn file_name(&self) -> Option<&str> {
    self.as_path().file_name()
  }

  pub fn file_stem(&self) -> Option<&str> {
    self.as_path().file_stem()
  }

  pub fn extension(&self) -> Option<&str> {
    self.as_path().extension()
  }

  pub fn is_absolute(&self) -> bool {
    self.as_path().is_absolute()
  }

  pub fn components(&self) -> Components<'_> {
    self.as_path().components()
  }

  pub fn starts_with<S: AsRef<str>>(&self, base: S) -> bool {
    self.as_path().starts_with(base)
  }

  pub fn ends_with<S: AsRef<str>>(&self, child: S) -> bool {
    self.as_path().ends_with(child)
  }

  pub fn strip_prefix<S: AsRef<str>>(&self, base: S) -> Result<ResolverPath<'_>, StripPrefixError> {
    self.as_path().strip_prefix(base)
  }

  // -- mutation --------------------------------------------------------------

  pub fn push<S: AsRef<str>>(&mut self, other: S) {
    let other = ResolverPath::new(other.as_ref());
    if other.is_absolute() {
      self.inner.clear();
      self.inner.push_str(other.inner);
      self.rehash();
      return;
    }
    let needs_sep = !self.inner.is_empty()
      && !self
        .inner
        .as_bytes()
        .last()
        .copied()
        .is_some_and(is_sep_byte);
    if needs_sep {
      self.inner.push(MAIN_SEPARATOR);
    }
    self.inner.push_str(other.inner);
    self.rehash();
  }

  pub fn pop(&mut self) -> bool {
    let Some(parent) = self.as_path().parent() else {
      return false;
    };
    let new_len = parent.inner.len();
    self.inner.truncate(new_len);
    self.rehash();
    true
  }

  pub fn set_file_name(&mut self, file_name: &str) {
    if self.file_name().is_some() {
      let popped = self.pop();
      debug_assert!(popped);
    }
    self.push(file_name);
  }

  pub fn set_extension(&mut self, new_ext: &str) -> bool {
    let Some(file_name) = self.file_name() else {
      return false;
    };
    let stem_len = rsplit_file_at_dot(file_name).0.len();
    let file_name_start = self.inner.len() - file_name.len();
    self.inner.truncate(file_name_start + stem_len);
    if !new_ext.is_empty() {
      self.inner.push('.');
      self.inner.push_str(new_ext);
    }
    self.rehash();
    true
  }
}

impl AsRef<str> for ResolverPathBuf {
  fn as_ref(&self) -> &str {
    &self.inner
  }
}

impl From<String> for ResolverPathBuf {
  fn from(s: String) -> Self {
    Self::from_string(s)
  }
}

impl From<&str> for ResolverPathBuf {
  fn from(s: &str) -> Self {
    Self::from_string(s.to_string())
  }
}

impl From<ResolverPath<'_>> for ResolverPathBuf {
  fn from(p: ResolverPath<'_>) -> Self {
    p.to_path_buf()
  }
}

impl From<ResolverPathBuf> for String {
  fn from(p: ResolverPathBuf) -> Self {
    p.into_string()
  }
}

impl fmt::Debug for ResolverPathBuf {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Debug::fmt(&self.inner, f)
  }
}

impl fmt::Display for ResolverPathBuf {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Display::fmt(&self.inner, f)
  }
}

impl PartialEq for ResolverPathBuf {
  fn eq(&self, other: &Self) -> bool {
    self.as_path() == other.as_path()
  }
}
impl Eq for ResolverPathBuf {}

impl PartialOrd for ResolverPathBuf {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}
impl Ord for ResolverPathBuf {
  fn cmp(&self, other: &Self) -> Ordering {
    self.as_path().cmp(&other.as_path())
  }
}

impl Hash for ResolverPathBuf {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.prehash.hash(state);
  }
}

// ---------------------------------------------------------------------------
// Ancestors / StripPrefixError / helpers
// ---------------------------------------------------------------------------

pub struct Ancestors<'a> {
  next: Option<ResolverPath<'a>>,
}

impl<'a> Iterator for Ancestors<'a> {
  type Item = ResolverPath<'a>;
  fn next(&mut self) -> Option<Self::Item> {
    let cur = self.next.take()?;
    self.next = cur.parent();
    Some(cur)
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StripPrefixError(());

impl fmt::Display for StripPrefixError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str("prefix not found")
  }
}

impl std::error::Error for StripPrefixError {}

fn components_equal(a: Component<'_>, b: Component<'_>) -> bool {
  match (a, b) {
    (Component::RootDir, Component::RootDir)
    | (Component::CurDir, Component::CurDir)
    | (Component::ParentDir, Component::ParentDir) => true,
    (Component::Prefix(x), Component::Prefix(y)) => {
      #[cfg(windows)]
      {
        x.eq_ignore_ascii_case(y)
      }
      #[cfg(not(windows))]
      {
        x == y
      }
    }
    (Component::Normal(x), Component::Normal(y)) => x == y,
    _ => false,
  }
}

fn iter_starts_with<'a, 'b>(mut a: Components<'a>, mut b: Components<'b>) -> bool {
  loop {
    match (a.next(), b.next()) {
      (_, None) => return true,
      (Some(x), Some(y)) if components_equal(x, y) => continue,
      _ => return false,
    }
  }
}

fn iter_ends_with<'a, 'b>(a: Components<'a>, b: Components<'b>) -> bool {
  let a: Vec<_> = a.collect();
  let b: Vec<_> = b.collect();
  if b.len() > a.len() {
    return false;
  }
  let start = a.len() - b.len();
  a[start..]
    .iter()
    .zip(b.iter())
    .all(|(x, y)| components_equal(*x, *y))
}

fn rsplit_file_at_dot(file: &str) -> (&str, Option<&str>) {
  if file == ".." {
    return (file, None);
  }
  match file.rfind('.') {
    Some(0) | None => (file, None),
    Some(i) => (&file[..i], Some(&file[i + 1..])),
  }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use std::path::Path;

  use super::*;

  fn p(s: &str) -> ResolverPath<'_> {
    ResolverPath::new(s)
  }

  fn agrees_with_std(s: &str) {
    let our = p(s);
    let std_p = Path::new(s);
    assert_eq!(
      our.parent().map(|p| p.as_str()),
      std_p.parent().and_then(|p| p.to_str()),
      "parent mismatch for {s:?}",
    );
    assert_eq!(
      our.file_name(),
      std_p.file_name().and_then(|n| n.to_str()),
      "file_name mismatch for {s:?}",
    );
    assert_eq!(
      our.file_stem(),
      std_p.file_stem().and_then(|n| n.to_str()),
      "file_stem mismatch for {s:?}",
    );
    assert_eq!(
      our.extension(),
      std_p.extension().and_then(|n| n.to_str()),
      "extension mismatch for {s:?}",
    );
    assert_eq!(
      our.is_absolute(),
      std_p.is_absolute(),
      "is_absolute mismatch for {s:?}",
    );
  }

  #[test]
  fn matches_std_path_basics() {
    let inputs = [
      "/",
      "/foo",
      "/foo/bar",
      "/foo/bar.js",
      "/foo/bar.d.ts",
      "/foo/.hidden",
      "/foo/..",
      "/foo/.",
      "/foo/bar/",
      "foo",
      "foo/bar",
      "./foo",
      "../foo",
      "",
      ".",
      "..",
    ];
    for s in inputs {
      agrees_with_std(s);
    }
  }

  #[test]
  #[cfg(windows)]
  fn matches_std_path_windows() {
    let inputs = [
      r"C:",
      r"C:\",
      r"C:\foo",
      r"C:\foo\bar.js",
      r"C:/foo/bar.js",
      r"\\server\share",
      r"\\server\share\path\file.js",
      r"\\?\C:\foo\bar",
    ];
    for s in inputs {
      agrees_with_std(s);
    }
  }

  #[test]
  fn join_basic() {
    assert_eq!(
      p("/foo").join("bar").as_str(),
      &format!("/foo{MAIN_SEPARATOR}bar")
    );
    assert_eq!(p("/foo/").join("bar").as_str(), "/foo/bar");
    assert_eq!(p("/foo").join("/bar").as_str(), "/bar");
    assert_eq!(p("").join("bar").as_str(), "bar");
  }

  #[test]
  fn pop_and_parent() {
    let mut buf = ResolverPathBuf::from("/foo/bar");
    assert!(buf.pop());
    assert_eq!(buf.as_str(), "/foo");
    assert!(buf.pop());
    assert_eq!(buf.as_str(), "/");
    assert!(!buf.pop());
  }

  #[test]
  fn starts_with_component_aware() {
    assert!(p("/foo/bar").starts_with("/foo"));
    assert!(!p("/foobar").starts_with("/foo"));
    assert!(p("/foo/bar/baz").starts_with("/foo/bar"));
  }

  #[test]
  fn strip_prefix_returns_tail() {
    let tail = p("/foo/bar/baz").strip_prefix("/foo").unwrap();
    assert_eq!(tail.as_str(), "bar/baz");
  }

  #[test]
  fn set_extension_works() {
    let mut buf = ResolverPathBuf::from("/foo/bar.js");
    assert!(buf.set_extension("ts"));
    assert_eq!(buf.as_str(), "/foo/bar.ts");
    assert!(buf.set_extension(""));
    assert_eq!(buf.as_str(), "/foo/bar");
  }

  #[test]
  fn ancestors_walk_up() {
    let chain: Vec<&str> = p("/foo/bar/baz").ancestors().map(|a| a.as_str()).collect();
    assert_eq!(chain, vec!["/foo/bar/baz", "/foo/bar", "/foo", "/"]);
  }

  #[test]
  fn prehash_consistent_borrow_vs_owned() {
    for s in ["", "/", "/foo", "/foo/bar/baz.js", "C:\\Users\\rust"] {
      let buf = ResolverPathBuf::from(s);
      let borrow = ResolverPath::new(s);
      assert_eq!(buf.precomputed_hash(), borrow.precomputed_hash(), "{s:?}");
      // as_path() forwards the prehash without recomputing.
      assert_eq!(buf.as_path().precomputed_hash(), buf.precomputed_hash());
    }
  }

  #[test]
  fn prehash_updates_with_mutation() {
    let mut buf = ResolverPathBuf::from("/foo");
    let h1 = buf.precomputed_hash();
    buf.push("bar");
    let h2 = buf.precomputed_hash();
    assert_ne!(h1, h2);
    assert_eq!(h2, hash_path(buf.as_str()));

    buf.pop();
    assert_eq!(buf.precomputed_hash(), h1);

    let mut buf = ResolverPathBuf::from("/foo/bar.js");
    let before = buf.precomputed_hash();
    buf.set_extension("ts");
    assert_ne!(before, buf.precomputed_hash());
    assert_eq!(buf.as_str(), "/foo/bar.ts");
    assert_eq!(buf.precomputed_hash(), hash_path("/foo/bar.ts"));
  }
}
