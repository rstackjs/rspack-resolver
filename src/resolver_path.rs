//! UTF-8 path types that mirror [`std::path::Path`] / [`std::path::PathBuf`].
//!
//! Internally rspack-resolver always speaks UTF-8 — we never have to roundtrip
//! through `OsStr`. `ResolverPath` / `ResolverPathBuf` give us the ergonomics of `Path`
//! (`.parent()`, `.join()`, `.file_name()`, `.components()`, …) operating
//! directly on string slices, with no `Option<&str>` from `to_str()` and no
//! allocation through `OsString`.
//!
//! Semantics aim to match `std::path` on each platform:
//! - On `cfg(unix)` only `/` is a separator.
//! - On `cfg(windows)` both `/` and `\` are separators, drive letters
//!   (`C:`) and UNC roots (`\\server\share`) are recognized as `Prefix`
//!   components, and `\\?\…` verbatim paths are passed through.
//!
//! These types only live inside the crate; the public API stays
//! `String` / `&str`.

// This module deliberately mirrors `std::path` naming, so a few clippy lints
// that ordinarily catch over-eager API surface are suppressed.
#![allow(
  clippy::use_self,
  clippy::unnecessary_wraps,
  clippy::elidable_lifetime_names,
  clippy::needless_continue
)]

use std::{
  borrow::Borrow,
  cmp::Ordering,
  fmt,
  hash::{BuildHasher, BuildHasherDefault, Hash, Hasher},
  ops::Deref,
};

use rustc_hash::FxHasher;

/// FxHash of a path slice, matching the hash other parts of the crate
/// compute via `FxHasher::default()` + `str::hash`. Centralized so the
/// precomputed value on [`ResolverPathBuf`] and the on-the-fly value from
/// [`ResolverPath::compute_hash`] agree.
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
///
/// The returned slice `&path[..prefix_len]` is what `Component::Prefix` covers,
/// matching `std::path::Path`'s notion of a prefix.
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
        // Find `server` then `share`.
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
          // `\\?\UNC\server` is invalid; treat as no prefix.
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
// Component
// ---------------------------------------------------------------------------

/// Iterator over [`ResolverPath`] components — mirrors `std::path::Component`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Component<'a> {
  /// A windows-style prefix (drive letter, UNC, verbatim).
  Prefix(&'a str),
  /// The root separator (`/` on Unix, after the prefix on Windows).
  RootDir,
  /// `.`
  CurDir,
  /// `..`
  ParentDir,
  /// A non-empty path segment.
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

/// Iterator over the components of a [`ResolverPath`].
#[derive(Clone)]
pub struct Components<'a> {
  /// Remaining slice yet to be tokenized.
  rest: &'a str,
  /// Whether a `Prefix` is still pending (only set on the very first step).
  prefix: Option<&'a str>,
  /// Whether a `RootDir` is still pending after the optional prefix.
  has_root: bool,
  /// Verbatim paths (Windows `\\?\…`) only split on `\`, never on `/`.
  verbatim: bool,
  /// True once the front cursor has yielded any component. Used to decide
  /// whether a leading `.` segment yields `CurDir` (it does) or gets skipped
  /// (any `.` after a real segment / root / prefix is dropped).
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

  /// Borrow what's left as a single `&ResolverPath`. Used by `Path::components().as_path()`-style consumers.
  pub fn as_str(&self) -> &'a str {
    // We don't model the prefix/root in `rest`; for the resolver's usages
    // the trailing remainder is enough.
    self.rest
  }

  fn is_sep(&self, b: u8) -> bool {
    if self.verbatim {
      is_verbatim_sep_byte(b)
    } else {
      is_sep_byte(b)
    }
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
      // Skip leading separators.
      while let Some((&b, tail)) = self.rest.as_bytes().split_first() {
        if self.is_sep(b) {
          // SAFETY: `tail` is the byte tail of a UTF-8 string after splitting at an ASCII separator,
          // so it remains valid UTF-8.
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
        // `.` is only kept if nothing has been emitted before it — i.e. it's a
        // leading `./`. Any other `.` segment is normalized away.
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
      // Trim trailing separators.
      while let Some((&b, head)) = self.rest.as_bytes().split_last() {
        if self.is_sep(b) {
          // SAFETY: `head` is the byte prefix of a UTF-8 string after splitting at an ASCII separator,
          // so it remains valid UTF-8.
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
            // Leading `.` only survives when nothing precedes it (no prefix,
            // no root, no earlier segment).
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
// ResolverPath (unsized)
// ---------------------------------------------------------------------------

/// Borrowed UTF-8 path. Mirror of [`std::path::Path`].
#[repr(transparent)]
pub struct ResolverPath {
  inner: str,
}

impl ResolverPath {
  /// Wrap a string slice as a path slice. Always succeeds — UTF-8 by construction.
  #[inline]
  pub fn new<S: AsRef<str> + ?Sized>(s: &S) -> &ResolverPath {
    // SAFETY: `ResolverPath` is `#[repr(transparent)]` around `str`.
    unsafe { &*(s.as_ref() as *const str as *const ResolverPath) }
  }

  /// View the path as a `&str`.
  #[inline]
  pub fn as_str(&self) -> &str {
    &self.inner
  }

  pub fn is_empty(&self) -> bool {
    self.inner.is_empty()
  }

  pub fn to_str(&self) -> Option<&str> {
    Some(&self.inner)
  }

  /// True if the path starts with a root (`/`, drive letter, UNC, …).
  pub fn is_absolute(&self) -> bool {
    let plen = prefix_len(&self.inner);
    let after = &self.inner.as_bytes()[plen..];
    #[cfg(windows)]
    {
      // On Windows a path is absolute when there is both a prefix *and* a root,
      // or when the prefix itself is verbatim/UNC (which implies a root).
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

  pub fn is_relative(&self) -> bool {
    !self.is_absolute()
  }

  /// Iterator over path components.
  pub fn components(&self) -> Components<'_> {
    Components::new(&self.inner)
  }

  /// Parent path. Returns `None` for a path that is just a prefix and/or root.
  pub fn parent(&self) -> Option<&ResolverPath> {
    let mut comps = self.components();
    let last = comps.next_back()?;
    match last {
      Component::Normal(_) | Component::CurDir | Component::ParentDir => {
        // After consuming the trailing segment, the unread byte length covers
        // exactly the parent (prefix + root + remaining `rest`).
        let mut end = comps.remaining_len();
        // Trim separators that sat between the parent and the just-removed
        // trailing segment, *but* keep the root separator itself.
        let root_keep = prefix_len(&self.inner) + usize::from(self.has_root_separator());
        while end > root_keep {
          let b = self.inner.as_bytes()[end - 1];
          if comps.is_sep(b) {
            end -= 1;
          } else {
            break;
          }
        }
        Some(ResolverPath::new(&self.inner[..end]))
      }
      Component::Prefix(_) | Component::RootDir => None,
    }
  }

  /// File name = the last `Normal` component, or `None`.
  pub fn file_name(&self) -> Option<&str> {
    self.components().next_back().and_then(|c| match c {
      Component::Normal(s) => Some(s),
      _ => None,
    })
  }

  /// File stem = file_name with the final extension stripped.
  pub fn file_stem(&self) -> Option<&str> {
    let name = self.file_name()?;
    Some(rsplit_file_at_dot(name).0)
  }

  /// File extension = the chars after the final `.` in `file_name()`, when present.
  pub fn extension(&self) -> Option<&str> {
    let name = self.file_name()?;
    rsplit_file_at_dot(name).1
  }

  /// Component-aware prefix check.
  pub fn starts_with<P: AsRef<ResolverPath>>(&self, base: P) -> bool {
    iter_starts_with(self.components(), base.as_ref().components())
  }

  /// Component-aware suffix check.
  pub fn ends_with<P: AsRef<ResolverPath>>(&self, child: P) -> bool {
    iter_ends_with(self.components(), child.as_ref().components())
  }

  /// Strip the given component-aware prefix, returning the tail.
  pub fn strip_prefix<P: AsRef<ResolverPath>>(
    &self,
    base: P,
  ) -> Result<&ResolverPath, StripPrefixError> {
    let base = base.as_ref();
    let mut s_comps = self.components();
    let mut b_comps = base.components();
    loop {
      match b_comps.next() {
        None => {
          // Consume any leading separators before returning.
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

  /// Join with another path. Absolute `other` replaces this path.
  pub fn join<P: AsRef<ResolverPath>>(&self, other: P) -> ResolverPathBuf {
    let mut buf = self.to_path_buf();
    buf.push(other);
    buf
  }

  /// Path with the extension swapped for `new_ext` (without leading dot).
  pub fn with_extension(&self, new_ext: &str) -> ResolverPathBuf {
    let mut buf = self.to_path_buf();
    buf.set_extension(new_ext);
    buf
  }

  /// Path with the file name swapped.
  pub fn with_file_name(&self, file_name: &str) -> ResolverPathBuf {
    let mut buf = self.to_path_buf();
    buf.set_file_name(file_name);
    buf
  }

  /// Ancestors iterator (self, self.parent(), self.parent().parent(), …).
  pub fn ancestors(&self) -> Ancestors<'_> {
    Ancestors { next: Some(self) }
  }

  /// Allocate an owned copy.
  pub fn to_path_buf(&self) -> ResolverPathBuf {
    ResolverPathBuf::from(self.inner.to_string())
  }

  /// FxHash of the underlying string slice.
  ///
  /// Borrowed [`ResolverPath`] values can't carry the precomputed hash that
  /// [`ResolverPathBuf::precomputed_hash`] returns (the type is unsized — no
  /// place to store it), so this recomputes on demand. The result agrees with
  /// the precomputed hash carried by [`ResolverPathBuf`] for the same bytes.
  #[inline]
  pub fn compute_hash(&self) -> u64 {
    hash_path(&self.inner)
  }

  /// Display in the platform's native form (forwarding to the underlying str).
  pub fn display(&self) -> &str {
    &self.inner
  }

  // -- internal helpers --

  fn has_root_separator(&self) -> bool {
    let plen = prefix_len(&self.inner);
    matches!(self.inner.as_bytes().get(plen), Some(&b) if is_sep_byte(b))
  }
}

impl<'a> Components<'a> {
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

fn components_equal(a: Component<'_>, b: Component<'_>) -> bool {
  match (a, b) {
    (Component::RootDir, Component::RootDir)
    | (Component::CurDir, Component::CurDir)
    | (Component::ParentDir, Component::ParentDir) => true,
    (Component::Prefix(x), Component::Prefix(y)) => {
      // On Windows, drive-letter prefixes are case-insensitive in std::path.
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

/// Split a file name into (stem, ext) using the same rule as `std::path::Path`:
///   - if the name has no `.`, or the only `.` is the first byte, ext is `None`.
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
// Standard trait impls for ResolverPath
// ---------------------------------------------------------------------------

impl AsRef<str> for ResolverPath {
  fn as_ref(&self) -> &str {
    &self.inner
  }
}

impl AsRef<ResolverPath> for ResolverPath {
  fn as_ref(&self) -> &ResolverPath {
    self
  }
}

impl AsRef<ResolverPath> for str {
  fn as_ref(&self) -> &ResolverPath {
    ResolverPath::new(self)
  }
}

impl AsRef<ResolverPath> for String {
  fn as_ref(&self) -> &ResolverPath {
    ResolverPath::new(self)
  }
}

impl fmt::Debug for ResolverPath {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Debug::fmt(&self.inner, f)
  }
}

impl fmt::Display for ResolverPath {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Display::fmt(&self.inner, f)
  }
}

impl PartialEq for ResolverPath {
  fn eq(&self, other: &Self) -> bool {
    // Component-wise equality — matches std::path::Path behaviour.
    self.components().eq(other.components())
  }
}
impl Eq for ResolverPath {}

impl PartialOrd for ResolverPath {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}
impl Ord for ResolverPath {
  fn cmp(&self, other: &Self) -> Ordering {
    self.components().cmp(other.components())
  }
}

impl Hash for ResolverPath {
  fn hash<H: Hasher>(&self, state: &mut H) {
    for c in self.components() {
      c.as_str().hash(state);
    }
  }
}

impl ToOwned for ResolverPath {
  type Owned = ResolverPathBuf;
  fn to_owned(&self) -> ResolverPathBuf {
    self.to_path_buf()
  }
}

// ---------------------------------------------------------------------------
// ResolverPathBuf (owned)
// ---------------------------------------------------------------------------

/// Owned UTF-8 path. Mirror of [`std::path::PathBuf`].
///
/// Stores a precomputed FxHash of the contained string alongside the buffer.
/// Cache layers (notably [`crate::cache::Cache`]) re-use this via
/// [`Self::precomputed_hash`] so insertions and lookups don't have to re-hash
/// the path each time.
#[derive(Clone)]
pub struct ResolverPathBuf {
  /// Precomputed `FxHash` of `inner`. Kept in sync with `inner` by every
  /// mutating method.
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

  /// Construct from a string whose `FxHash` has already been computed elsewhere
  /// (e.g. by a cache that hashes the key for lookup before deciding to
  /// allocate). The caller is responsible for the hash matching the bytes; if
  /// `debug_assertions` is enabled we verify.
  #[inline]
  pub fn with_prehash(prehash: u64, s: String) -> Self {
    debug_assert_eq!(prehash, hash_path(&s), "prehash does not match string");
    Self { prehash, inner: s }
  }

  pub fn as_path(&self) -> &ResolverPath {
    ResolverPath::new(&self.inner)
  }

  pub fn as_str(&self) -> &str {
    &self.inner
  }

  pub fn into_string(self) -> String {
    self.inner
  }

  pub fn into_boxed_path(self) -> Box<ResolverPath> {
    let raw: *mut str = Box::into_raw(self.inner.into_boxed_str());
    // SAFETY: `ResolverPath` is `#[repr(transparent)]` around `str`.
    unsafe { Box::from_raw(raw as *mut ResolverPath) }
  }

  /// Precomputed FxHash of the contained path. Cheap to call (single field
  /// read) — use this when feeding identity-hashed maps/sets to avoid
  /// re-hashing the same path on every lookup.
  ///
  /// The value agrees with [`ResolverPath::compute_hash`] for the same path
  /// bytes, so a borrowed `&ResolverPath` and an owned `ResolverPathBuf`
  /// produce identical hashes.
  #[inline]
  pub fn precomputed_hash(&self) -> u64 {
    self.prehash
  }

  /// Refresh `prehash` after a mutation. Centralized so `push`/`pop`/
  /// `set_file_name`/`set_extension` can't forget to keep it in sync.
  #[inline]
  fn rehash(&mut self) {
    self.prehash = hash_path(&self.inner);
  }

  /// Append `other`. Absolute `other` replaces the current contents (mirrors
  /// `PathBuf::push`).
  pub fn push<P: AsRef<ResolverPath>>(&mut self, other: P) {
    let other = other.as_ref();
    if other.is_absolute() {
      self.inner.clear();
      self.inner.push_str(&other.inner);
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
    self.inner.push_str(&other.inner);
    self.rehash();
  }

  /// Remove the final component. Returns `false` if there was nothing to pop.
  pub fn pop(&mut self) -> bool {
    let Some(parent) = self.as_path().parent() else {
      return false;
    };
    let new_len = parent.inner.len();
    self.inner.truncate(new_len);
    self.rehash();
    true
  }

  /// Replace the final component's file name.
  pub fn set_file_name(&mut self, file_name: &str) {
    if self.as_path().file_name().is_some() {
      let popped = self.pop();
      debug_assert!(popped);
    }
    self.push(ResolverPath::new(file_name));
  }

  /// Replace (or set) the file extension.
  pub fn set_extension(&mut self, new_ext: &str) -> bool {
    let Some(file_name) = self.as_path().file_name() else {
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

impl Deref for ResolverPathBuf {
  type Target = ResolverPath;
  fn deref(&self) -> &Self::Target {
    self.as_path()
  }
}

impl Borrow<ResolverPath> for ResolverPathBuf {
  fn borrow(&self) -> &ResolverPath {
    self.as_path()
  }
}

impl AsRef<ResolverPath> for ResolverPathBuf {
  fn as_ref(&self) -> &ResolverPath {
    self.as_path()
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

impl From<&ResolverPath> for ResolverPathBuf {
  fn from(p: &ResolverPath) -> Self {
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
    self.as_path().cmp(other.as_path())
  }
}

impl Hash for ResolverPathBuf {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.as_path().hash(state);
  }
}

// ---------------------------------------------------------------------------
// Ancestors / StripPrefixError
// ---------------------------------------------------------------------------

pub struct Ancestors<'a> {
  next: Option<&'a ResolverPath>,
}

impl<'a> Iterator for Ancestors<'a> {
  type Item = &'a ResolverPath;
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
  use std::path::Path;

  use super::*;

  fn p(s: &str) -> &ResolverPath {
    ResolverPath::new(s)
  }

  /// Assert that two paths agree on a set of properties with `std::path::Path`.
  fn agrees_with_std(s: &str) {
    let our = p(s);
    let std_p = Path::new(s);
    assert_eq!(
      our.parent().map(ResolverPath::as_str),
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
    // Trailing separator preserved (no double separator).
    assert_eq!(p("/foo/").join("bar").as_str(), "/foo/bar");
    // Absolute `other` replaces base.
    assert_eq!(p("/foo").join("/bar").as_str(), "/bar");
    // Empty base.
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
    let chain: Vec<&str> = p("/foo/bar/baz")
      .ancestors()
      .map(ResolverPath::as_str)
      .collect();
    assert_eq!(chain, vec!["/foo/bar/baz", "/foo/bar", "/foo", "/"]);
  }

  #[test]
  fn box_path_roundtrip() {
    let buf = ResolverPathBuf::from("/foo/bar");
    let boxed: Box<ResolverPath> = buf.into_boxed_path();
    assert_eq!(boxed.as_str(), "/foo/bar");
  }

  #[test]
  fn prehash_matches_borrowed_compute_hash() {
    for s in ["", "/", "/foo", "/foo/bar/baz.js", "C:\\Users\\rust"] {
      let buf = ResolverPathBuf::from(s);
      assert_eq!(
        buf.precomputed_hash(),
        ResolverPath::new(s).compute_hash(),
        "hashes disagree for {s:?}"
      );
    }
  }

  #[test]
  fn prehash_updates_with_mutation() {
    let mut buf = ResolverPathBuf::from("/foo");
    let h1 = buf.precomputed_hash();
    buf.push(ResolverPath::new("bar"));
    let h2 = buf.precomputed_hash();
    assert_ne!(h1, h2);
    assert_eq!(h2, ResolverPath::new(buf.as_str()).compute_hash());

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
