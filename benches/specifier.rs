//! Microbenchmarks for `Specifier::parse`.
//!
//! Kept in a separate bench binary from `bench_resolver` for measurement
//! stability: each `[[bench]]` runs in its own process, so the short
//! `specifier/*` cases get a fresh, predictable instruction cache instead of
//! competing with the much larger resolver bench code for cache lines. This
//! keeps cold-start cache misses out of the per-case CodSpeed deltas.

#[cfg(target_family = "wasm")]
use std::alloc::System;
use std::alloc::{GlobalAlloc, Layout};

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rspack_resolver::__BenchSpecifier as Specifier;

#[global_allocator]
#[cfg(not(target_family = "wasm"))]
static GLOBAL: NeverGrowInPlaceAllocator<mimalloc::MiMalloc> =
  NeverGrowInPlaceAllocator::new(mimalloc::MiMalloc);

#[global_allocator]
#[cfg(target_family = "wasm")]
static GLOBAL: NeverGrowInPlaceAllocator<System> = NeverGrowInPlaceAllocator::new(System);

/// Mirrors the allocator wrapper in `bench_resolver` so allocation costs are
/// measured identically across both bench binaries. See `benches/resolver.rs`
/// for the rationale.
struct NeverGrowInPlaceAllocator<A> {
  allocator: A,
}

impl<A> NeverGrowInPlaceAllocator<A> {
  const fn new(allocator: A) -> Self {
    Self { allocator }
  }
}

// SAFETY: Methods simply delegate to the wrapped allocator.
unsafe impl<A: GlobalAlloc> GlobalAlloc for NeverGrowInPlaceAllocator<A> {
  unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
    self.allocator.alloc(layout)
  }

  unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
    self.allocator.dealloc(ptr, layout)
  }
}

// `path` is repeated to reach `len`, then optional `?query` / `#fragment`
// suffix is appended. Lets us scale a single shape across short/medium/long
// inputs without changing branch coverage.
fn specifier_shaped(base: &str, len: usize, query: Option<&str>, fragment: Option<&str>) -> String {
  let mut s = String::with_capacity(len + 64);
  while s.len() < len {
    s.push_str(base);
  }
  s.truncate(len);
  if let Some(q) = query {
    s.push('?');
    s.push_str(q);
  }
  if let Some(f) = fragment {
    s.push('#');
    s.push_str(f);
  }
  s
}

// Webpack-style synthesized escapes: inserts `\0#` sequences inside the path
// so the scanner hits the `prev == '\0'` branch and is forced onto the
// Cow::Owned slow path.
fn specifier_with_escapes(parts: &[&str]) -> String {
  parts.join("\0#")
}

fn specifier_branch_cases() -> Vec<(&'static str, String)> {
  vec![
    // 1. (None, None) — fast path, Cow::Borrowed
    ("none/short", "./foo.js".to_string()),
    (
      "none/medium",
      "./packages/utils/src/internal/helpers/normalizePath.ts".to_string(),
    ),
    // 2. (Some, None) — query only
    ("query/short", "./a.js?vue".to_string()),
    (
      "query/medium",
      "./Button.tsx?vue&type=script&lang=ts&scoped=true&hash=abc12345".to_string(),
    ),
    // 3. (None, Some) — fragment only, scanner breaks early
    ("fragment/short", "./a.js#main".to_string()),
    (
      "fragment/medium",
      "./pages/Home.tsx#section-introduction-to-the-rspack-resolver".to_string(),
    ),
    // 4. (Some, Some) — query then fragment
    ("query+fragment/short", "./a.js?x#y".to_string()),
    (
      "query+fragment/medium",
      "./Widget.vue?vue&type=template&lang=html#root".to_string(),
    ),
    // 5. multiple `?`, only first becomes query_start
    (
      "multi-question",
      "./a.js?one?two?three?four?five?six?seven".to_string(),
    ),
    // 6. `\0#` escape → slow path (Cow::Owned + char_indices filter)
    (
      "escape/single",
      specifier_with_escapes(&["path/", "real-hash"]),
    ),
    // 7. multiple escapes — repeats the slow path several times in one input
    (
      "escape/many",
      specifier_with_escapes(&["./pkg/", "repo", "repo2", "repo3", "repo4#hash"]),
    ),
    // 8. leading `/`, `.`, `#` → offset=1; the first char is skipped intentionally
    ("leading-slash", "/abs/path/to/file.mjs?q#f".to_string()),
    ("leading-hash", "#alias/module.cjs?q#f".to_string()),
    // 9. bare module — offset=0, scan starts at index 0
    (
      "bare-module",
      "@scope/package/sub/path/index.js?q#f".to_string(),
    ),
    // 10. `?` inside a fragment must NOT be promoted to query — scanner already
    //     broke at the `#`, but worth pinning so a future refactor can't regress it.
    (
      "fragment-with-question",
      "./a.js#frag?not-a-query&also-not".to_string(),
    ),
  ]
}

// Same shape, four sizes — measures how each branch scales with input length.
// Sizes chosen at ~5x steps so codspeed renders a clear curve.
const SPECIFIER_LENGTH_TIERS: &[(&str, usize)] = &[
  ("len_8", 8),
  ("len_64", 64),
  ("len_256", 256),
  ("len_1536", 1536),
];

#[allow(clippy::type_complexity)]
fn specifier_length_shapes() -> Vec<(
  &'static str,
  &'static str,
  Option<&'static str>,
  Option<&'static str>,
)> {
  vec![
    // Pure path: stresses the loop body without any branch hits.
    ("path-only", "./a/b/c/d/e/", None, None),
    // Query at the very end: full scan before query_start is set.
    (
      "query-tail",
      "./a/b/c/d/e/",
      Some("vue&type=script&lang=ts"),
      None,
    ),
    // Fragment at the very end: full scan, then early break at last char.
    ("frag-tail", "./a/b/c/d/e/", None, Some("section-end")),
    // Both at the tail.
    (
      "both-tail",
      "./a/b/c/d/e/",
      Some("vue&type=script"),
      Some("hash"),
    ),
  ]
}

// Hand-picked from typical rspack/webpack loader chains; these are what the
// parser actually sees in a production resolve flow.
fn specifier_realistic_cases() -> Vec<(&'static str, &'static str)> {
  vec![
    ("rw/loader-chain",
     "./node_modules/.pnpm/vue-loader@17.0.0/node_modules/vue-loader/dist/templateLoader.js?vue&type=template&id=2f8c6e7a&scoped=true&lang=html"),
    ("rw/css-modules",
     "./src/components/Sidebar/Sidebar.module.css?ngGlobalStyle&hash=d41d8cd98f00b204e9800998ecf8427e"),
    ("rw/asset-query",
     "./public/assets/images/hero@2x.png?as=webp&w=1920&h=1080&quality=80&format=webp"),
    ("rw/hash-only",
     "./shared/utils/index.ts#tree-shaken-export-marker-do-not-strip"),
    ("rw/inline-loader",
     "!!./node_modules/css-loader/dist/cjs.js??ref--6-oneOf-1-1!./node_modules/postcss-loader/dist/cjs.js??ref--6-oneOf-1-2!./src/App.vue?vue&type=style&index=0&id=7ba5bd90&scoped=true&lang=css"),
  ]
}

fn bench_specifier_branches(c: &mut Criterion) {
  let mut group = c.benchmark_group("specifier/branches");
  for (label, input) in specifier_branch_cases() {
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.bench_with_input(BenchmarkId::from_parameter(label), &input, |b, s| {
      b.iter(|| {
        let parsed = Specifier::parse(black_box(s.as_str())).unwrap();
        black_box(parsed);
      });
    });
  }
  group.finish();
}

fn bench_specifier_length_sweep(c: &mut Criterion) {
  let mut group = c.benchmark_group("specifier/length");
  for (shape_label, base, query, fragment) in specifier_length_shapes() {
    for (len_label, len) in SPECIFIER_LENGTH_TIERS {
      let input = specifier_shaped(base, *len, query, fragment);
      let id = BenchmarkId::new(shape_label, len_label);
      group.throughput(Throughput::Bytes(input.len() as u64));
      group.bench_with_input(id, &input, |b, s| {
        b.iter(|| {
          let parsed = Specifier::parse(black_box(s.as_str())).unwrap();
          black_box(parsed);
        });
      });
    }
  }
  group.finish();
}

fn bench_specifier_escape_scaling(c: &mut Criterion) {
  // Slow path scales with both input length AND the number of escapes (the
  // filter closure does an O(n*k) `escaped_indexes.contains(&i)` per char).
  // Worth a dedicated knob so the optimizer can target it.
  let mut group = c.benchmark_group("specifier/escapes");
  for &n in &[1usize, 4, 16, 64] {
    // `parts.len()` must equal `n + 1` so that `join("\0#")` inserts exactly
    // `n` separators (= `n` escape markers in the input). The first element
    // is a path prefix, the last is the real `#fragment`, and we pad the
    // middle with `n - 1` filler segments.
    let mut parts = vec!["./pkg/"];
    for _ in 0..n.saturating_sub(1) {
      parts.push("segment");
    }
    parts.push("real#hash");
    let input = specifier_with_escapes(&parts);
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.bench_with_input(
      BenchmarkId::from_parameter(format!("escapes_{n}")),
      &input,
      |b, s| {
        b.iter(|| {
          let parsed = Specifier::parse(black_box(s.as_str())).unwrap();
          black_box(parsed);
        });
      },
    );
  }
  group.finish();
}

fn bench_specifier_realistic(c: &mut Criterion) {
  let mut group = c.benchmark_group("specifier/realistic");
  for (label, input) in specifier_realistic_cases() {
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.bench_with_input(BenchmarkId::from_parameter(label), input, |b, s| {
      b.iter(|| {
        let parsed = Specifier::parse(black_box(s)).unwrap();
        black_box(parsed);
      });
    });
  }
  group.finish();
}

criterion_group!(
  specifier,
  bench_specifier_branches,
  bench_specifier_length_sweep,
  bench_specifier_escape_scaling,
  bench_specifier_realistic
);
criterion_main!(specifier);
