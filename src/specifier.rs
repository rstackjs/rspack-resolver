use std::borrow::Cow;

use crate::error::SpecifierError;

#[derive(Debug)]
pub struct Specifier<'a> {
  path: Cow<'a, str>,
  pub query: Option<&'a str>,
  pub fragment: Option<&'a str>,
}

impl<'a> Specifier<'a> {
  pub fn path(&'a self) -> &'a str {
    self.path.as_ref()
  }

  /// Parse a module specifier into path, query, and fragment.
  ///
  /// # Errors
  ///
  /// * See [SpecifierError]
  pub fn parse(specifier: &'a str) -> Result<Self, SpecifierError> {
    if specifier.is_empty() {
      return Err(SpecifierError::Empty(specifier.to_string()));
    }
    let offset = match specifier.as_bytes()[0] {
      b'/' | b'.' | b'#' => 1,
      _ => 0,
    };
    let (path, query, fragment) = Self::parse_query_framgment(specifier, offset);
    if path.is_empty() {
      return Err(SpecifierError::Empty(specifier.to_string()));
    }
    Ok(Self {
      path,
      query,
      fragment,
    })
  }

  fn parse_query_framgment(
    specifier: &'a str,
    skip: usize,
  ) -> (Cow<'a, str>, Option<&'a str>, Option<&'a str>) {
    let mut query_start: Option<usize> = None;
    let mut fragment_start: Option<usize> = None;
    let mut escaped_indexes: Vec<usize> = Vec::new();

    // Scan as bytes: `?`, `#`, and `\0` are single-byte ASCII (< 0x80), so their
    // byte positions coincide with char positions in any UTF-8 input. This skips
    // the per-step UTF-8 decode that `char_indices()` performs, which dominated
    // the callgrind baseline.
    let bytes = specifier.as_bytes();
    let mut prev = bytes[0];
    for (i, &b) in bytes.iter().enumerate().skip(skip) {
      if b == b'?' && query_start.is_none() {
        query_start = Some(i);
      }
      if b == b'#' {
        if prev == 0 {
          escaped_indexes.push(i - 1);
        } else {
          fragment_start = Some(i);
          break;
        }
      }
      prev = b;
    }

    let (path, query, fragment) = match (query_start, fragment_start) {
      (Some(i), Some(j)) => {
        debug_assert!(i < j);
        (
          &specifier[..i],
          Some(&specifier[i..j]),
          Some(&specifier[j..]),
        )
      }
      (Some(i), None) => (&specifier[..i], Some(&specifier[i..]), None),
      (None, Some(j)) => (&specifier[..j], None, Some(&specifier[j..])),
      _ => (specifier, None, None),
    };

    let path = if escaped_indexes.is_empty() {
      Cow::Borrowed(path)
    } else {
      // Each escaped index points at a `\0` byte that we need to drop. Copy the
      // surrounding slices in one pass — O(n) — instead of re-decoding chars and
      // calling `escaped_indexes.contains(&i)` per char, which was O(n*k).
      // The slice indices land on char boundaries because `\0` is ASCII.
      let mut s = String::with_capacity(path.len() - escaped_indexes.len());
      let mut last = 0;
      for &esc in &escaped_indexes {
        s.push_str(&path[last..esc]);
        last = esc + 1;
      }
      s.push_str(&path[last..]);
      Cow::Owned(s)
    };

    (path, query, fragment)
  }
}

/// Classification of a specifier's first character, used by the resolver to
/// dispatch a fresh specifier into the matching resolution path.
///
/// On Unix and for `/`-prefixed inputs on Windows this is equivalent to
/// inspecting `Path::new(specifier).components().next()` for `RootDir`,
/// `CurDir`, `ParentDir`, and `Normal` — but without running the full
/// `std::path::Components` state machine (Windows-prefix detection,
/// `Component` enum construction, UTF-8 boundary checks) on what is
/// effectively a 1- or 2-byte decision. The separator and `.`/`..` markers
/// are always single-byte ASCII, so the dispatch can be made by direct byte
/// inspection.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SpecifierHead {
  /// Starts with `/` (Unix root) or, on Windows, a drive/UNC prefix.
  Absolute,
  /// Starts with `./`, `../`, or is exactly `.` or `..`.
  Relative,
  /// Starts with `#` — package imports / subpath imports.
  Hash,
  /// Anything else — bare specifier, empty string, or starts with `.`
  /// followed by a non-`/` (e.g. `.foo`), which `Components` reports as a
  /// `Normal` segment.
  Bare,
}

/// Classify the first character of a module specifier.
///
/// See [`SpecifierHead`] for the contract and the property test in this
/// module's `tests` for the equivalence proof against the std API.
pub fn classify_specifier_head(specifier: &str) -> SpecifierHead {
  let bytes = specifier.as_bytes();
  match bytes.first() {
    None => SpecifierHead::Bare,
    Some(b'/') => SpecifierHead::Absolute,
    Some(b'#') => SpecifierHead::Hash,
    Some(b'.') => match bytes.get(1) {
      // `.` alone, or `./...`
      None | Some(b'/') => SpecifierHead::Relative,
      Some(b'.') => match bytes.get(2) {
        // `..` alone, or `../...`
        None | Some(b'/') => SpecifierHead::Relative,
        // `..foo` — `Components` reports this as `Normal("..foo")`.
        _ => SpecifierHead::Bare,
      },
      // `.foo` — `Components` reports as `Normal(".foo")`.
      _ => SpecifierHead::Bare,
    },
    #[cfg(windows)]
    Some(b'\\') => SpecifierHead::Absolute,
    _ => {
      // On Windows, drive-letter specifiers like `C:` are reported as
      // `Component::Prefix`. Defer to the std path API to stay correct.
      #[cfg(windows)]
      {
        use std::path::{Component, Path};
        if matches!(
          Path::new(specifier).components().next(),
          Some(Component::RootDir | Component::Prefix(_))
        ) {
          return SpecifierHead::Absolute;
        }
      }
      SpecifierHead::Bare
    }
  }
}

#[cfg(test)]
mod tests {
  use super::{classify_specifier_head, Specifier, SpecifierError, SpecifierHead};

  #[test]
  fn debug() {
    let specifier = Specifier::parse("/").unwrap();
    assert_eq!(
      format!("{specifier:?}"),
      r#"Specifier { path: "/", query: None, fragment: None }"#
    );
  }

  #[test]
  fn empty() {
    let specifiers = ["", "?"];
    for specifier in specifiers {
      let error = Specifier::parse(specifier).unwrap_err();
      assert_eq!(error, SpecifierError::Empty(specifier.to_string()));
    }
  }

  #[test]
  fn absolute() -> Result<(), SpecifierError> {
    let specifier = "/test?#";
    let parsed = Specifier::parse(specifier)?;
    assert_eq!(parsed.path, "/test");
    assert_eq!(parsed.query, Some("?"));
    assert_eq!(parsed.fragment, Some("#"));
    Ok(())
  }

  #[test]
  fn relative() -> Result<(), SpecifierError> {
    let specifiers = ["./test", "../test", "../../test"];
    for specifier in specifiers {
      let mut r = specifier.to_string();
      r.push_str("?#");
      let parsed = Specifier::parse(&r)?;
      assert_eq!(parsed.path, specifier);
      assert_eq!(parsed.query, Some("?"));
      assert_eq!(parsed.fragment, Some("#"));
    }
    Ok(())
  }

  #[test]
  fn hash() -> Result<(), SpecifierError> {
    let specifiers = ["#", "#path"];
    for specifier in specifiers {
      let mut r = specifier.to_string();
      r.push_str("?#");
      let parsed = Specifier::parse(&r)?;
      assert_eq!(parsed.path, specifier);
      assert_eq!(parsed.query, Some("?"));
      assert_eq!(parsed.fragment, Some("#"));
    }
    Ok(())
  }

  #[test]
  fn module() -> Result<(), SpecifierError> {
    let specifiers = ["module"];
    for specifier in specifiers {
      let mut r = specifier.to_string();
      r.push_str("?#");
      let parsed = Specifier::parse(&r)?;
      assert_eq!(parsed.path, specifier);
      assert_eq!(parsed.query, Some("?"));
      assert_eq!(parsed.fragment, Some("#"));
    }
    Ok(())
  }

  #[test]
  fn query_fragment() -> Result<(), SpecifierError> {
    let data = [
      ("a?", Some("?"), None),
      ("a?query", Some("?query"), None),
      ("a?query1?query2", Some("?query1?query2"), None),
      (
        "a?query1?query2?query3",
        Some("?query1?query2?query3"),
        None,
      ),
      ("a#", None, Some("#")),
      ("a#b#c", None, Some("#b#c")),
      ("a#fragment", None, Some("#fragment")),
      ("a?#", Some("?"), Some("#")),
      ("a?#fragment", Some("?"), Some("#fragment")),
      ("a?query#", Some("?query"), Some("#")),
      ("a?query#fragment", Some("?query"), Some("#fragment")),
      ("a#fragment?", None, Some("#fragment?")),
      ("a#fragment?query", None, Some("#fragment?query")),
    ];

    for (specifier_str, query, fragment) in data {
      let specifier = Specifier::parse(specifier_str)?;
      assert_eq!(specifier.path, "a", "{specifier_str}");
      assert_eq!(specifier.query, query, "{specifier_str}");
      assert_eq!(specifier.fragment, fragment, "{specifier_str}");
    }

    Ok(())
  }

  #[test]
  // https://github.com/webpack/enhanced-resolve/blob/main/test/identifier.test.js
  fn enhanced_resolve_edge_cases() -> Result<(), SpecifierError> {
    let data = [
      ("path/#", "path/", "", "#"),
      ("path/as/?", "path/as/", "?", ""),
      ("path/#/?", "path/", "", "#/?"),
      ("path/#repo#hash", "path/", "", "#repo#hash"),
      ("path/#r#hash", "path/", "", "#r#hash"),
      ("path/#repo/#repo2#hash", "path/", "", "#repo/#repo2#hash"),
      ("path/#r/#r#hash", "path/", "", "#r/#r#hash"),
      (
        "path/#/not/a/hash?not-a-query",
        "path/",
        "",
        "#/not/a/hash?not-a-query",
      ),
    ];

    for (specifier_str, path, query, fragment) in data {
      let specifier = Specifier::parse(specifier_str)?;
      assert_eq!(specifier.path, path, "{specifier_str}");
      assert_eq!(specifier.query.unwrap_or(""), query, "{specifier_str}");
      assert_eq!(
        specifier.fragment.unwrap_or(""),
        fragment,
        "{specifier_str}"
      );
    }

    Ok(())
  }

  // https://github.com/webpack/enhanced-resolve/blob/main/test/identifier.test.js
  #[test]
  fn enhanced_resolve_windows_like() -> Result<(), SpecifierError> {
    let data = [
      ("path\\#", "path\\", "", "#"),
      ("path\\as\\?", "path\\as\\", "?", ""),
      ("path\\#\\?", "path\\", "", "#\\?"),
      ("path\\#repo#hash", "path\\", "", "#repo#hash"),
      ("path\\#r#hash", "path\\", "", "#r#hash"),
      (
        "path\\#repo\\#repo2#hash",
        "path\\",
        "",
        "#repo\\#repo2#hash",
      ),
      ("path\\#r\\#r#hash", "path\\", "", "#r\\#r#hash"),
      (
        "path\\#/not/a/hash?not-a-query",
        "path\\",
        "",
        "#/not/a/hash?not-a-query",
      ),
    ];

    for (specifier_str, path, query, fragment) in data {
      let specifier = Specifier::parse(specifier_str)?;
      assert_eq!(specifier.path, path, "{specifier_str}");
      assert_eq!(specifier.query.unwrap_or(""), query, "{specifier_str}");
      assert_eq!(
        specifier.fragment.unwrap_or(""),
        fragment,
        "{specifier_str}"
      );
    }

    Ok(())
  }

  /// Reference dispatch derived from `Path::new(s).components().next()`. This
  /// is the algorithm `classify_specifier_head` must remain equivalent to on
  /// Unix and for `/`-prefixed inputs on Windows.
  fn reference_head(specifier: &str) -> SpecifierHead {
    use std::path::{Component, Path};
    match Path::new(specifier).components().next() {
      Some(Component::RootDir | Component::Prefix(_)) => SpecifierHead::Absolute,
      Some(Component::CurDir | Component::ParentDir) => SpecifierHead::Relative,
      Some(Component::Normal(_)) if specifier.as_bytes().first() == Some(&b'#') => {
        SpecifierHead::Hash
      }
      _ => SpecifierHead::Bare,
    }
  }

  #[test]
  fn classify_specifier_head_known_cases() {
    let cases = [
      ("", SpecifierHead::Bare),
      ("/abs", SpecifierHead::Absolute),
      ("/", SpecifierHead::Absolute),
      (".", SpecifierHead::Relative),
      ("..", SpecifierHead::Relative),
      ("./foo", SpecifierHead::Relative),
      ("../foo", SpecifierHead::Relative),
      (".foo", SpecifierHead::Bare),
      ("..foo", SpecifierHead::Bare),
      ("#imports/sub", SpecifierHead::Hash),
      ("react", SpecifierHead::Bare),
      ("@scope/pkg", SpecifierHead::Bare),
      ("中文/包", SpecifierHead::Bare),
    ];
    for (input, expected) in cases {
      assert_eq!(classify_specifier_head(input), expected, "{input:?}");
      assert_eq!(reference_head(input), expected, "{input:?} reference");
    }
  }

  proptest::proptest! {
    /// Verify `classify_specifier_head` matches the std `Path::components`-based
    /// dispatch for any ASCII input (the universe the resolver actually sees).
    #[test]
    fn classify_matches_components_for_ascii(s in "[\\x20-\\x7e]{0,32}") {
      proptest::prop_assert_eq!(classify_specifier_head(&s), reference_head(&s));
    }

    /// Same equivalence over arbitrary UTF-8 to cover non-ASCII segment bytes.
    /// Skipped on Windows, where drive/UNC prefixes (e.g. `C:`) bypass the
    /// fast path and need std parsing.
    #[cfg(unix)]
    #[test]
    fn classify_matches_components_for_utf8(s in ".*") {
      proptest::prop_assert_eq!(classify_specifier_head(&s), reference_head(&s));
    }
  }
}
