#[cfg(target_family = "wasm")]
use std::alloc::System;
use std::{
  alloc::{GlobalAlloc, Layout},
  env, fs,
  fs::read_to_string,
  future::Future,
  io::{self, Write},
  path::{Path, PathBuf},
  sync::Arc,
};

#[global_allocator]
#[cfg(not(target_family = "wasm"))]
static GLOBAL: NeverGrowInPlaceAllocator<mimalloc::MiMalloc> =
  NeverGrowInPlaceAllocator::new(mimalloc::MiMalloc);

#[global_allocator]
#[cfg(target_family = "wasm")]
static GLOBAL: NeverGrowInPlaceAllocator<System> = NeverGrowInPlaceAllocator::new(System);

/// Delegates `alloc`/`dealloc` to the wrapped allocator but omits
/// [`GlobalAlloc::realloc`], forcing the default "alloc-new + copy + dealloc-old"
/// path so that benchmarks never benefit from non-deterministic in-place growth
/// provided by the underlying allocator's `realloc`. Wrapping `mimalloc::MiMalloc`
/// (instead of using it directly) also keeps `alloc` / `dealloc` visible to
/// CodSpeed's mimalloc white-box allocator tracking.
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

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rspack_resolver::{
  FileSystemOptions, FileSystemOs, ResolveOptions, Resolver, __BenchSpecifier as Specifier,
};
use serde_json::Value;
use tokio::{
  runtime::{self, Builder},
  task::JoinSet,
};

fn symlink<P: AsRef<Path>, Q: AsRef<Path>>(original: P, link: Q) -> io::Result<()> {
  #[cfg(target_family = "unix")]
  {
    std::os::unix::fs::symlink(original, link)
  }

  #[cfg(target_family = "windows")]
  {
    std::os::windows::fs::symlink_file(original, link)
  }
}

fn create_symlinks() -> io::Result<PathBuf> {
  let root = env::current_dir()?.join("fixtures/enhanced_resolve");
  let dirname = root.join("test");
  let temp_path = dirname.join("temp_symlinks");
  let create_symlink_fixtures = || -> io::Result<()> {
    fs::create_dir(&temp_path)?;
    let mut index = fs::File::create(temp_path.join("index.js"))?;
    index.write_all(b"console.log('Hello, World!')")?;
    // create 10000 symlink files pointing to the index.js
    for i in 0..10000 {
      symlink(
        temp_path.join("index.js"),
        temp_path.join(format!("file{i}.js")),
      )?;
    }
    Ok(())
  };
  if !temp_path.exists() {
    if let Err(err) = create_symlink_fixtures() {
      let _ = fs::remove_dir_all(&temp_path);
      return Err(err);
    }
  }
  Ok(temp_path)
}

fn rspack_resolver(enable_pnp: bool) -> rspack_resolver::Resolver {
  use rspack_resolver::{AliasValue, ResolveOptions, Resolver};
  let alias_value = AliasValue::from("./");

  let fs = FileSystemOs::new(FileSystemOptions {
    #[cfg(feature = "yarn_pnp")]
    enable_pnp,
  });

  Resolver::new_with_file_system(
    fs,
    ResolveOptions {
      #[cfg(feature = "yarn_pnp")]
      enable_pnp,
      extensions: vec![".ts".into(), ".js".into(), ".mjs".into()],
      condition_names: vec!["import".into(), "webpack".into(), "require".into()],
      alias_fields: vec![vec!["browser".into()]],
      extension_alias: vec![(".js".into(), vec![".ts".into(), ".js".into()])],
      // Real projects LOVE setting these many aliases.
      // I saw them with my own eyes.
      alias: vec![
        ("/absolute/path".into(), vec![alias_value.clone()]),
        ("aaa".into(), vec![alias_value.clone()]),
        ("bbb".into(), vec![alias_value.clone()]),
        ("ccc".into(), vec![alias_value.clone()]),
        ("ddd".into(), vec![alias_value.clone()]),
        ("eee".into(), vec![alias_value.clone()]),
        ("fff".into(), vec![alias_value.clone()]),
        ("ggg".into(), vec![alias_value.clone()]),
        ("hhh".into(), vec![alias_value.clone()]),
        ("iii".into(), vec![alias_value.clone()]),
        ("jjj".into(), vec![alias_value.clone()]),
        ("kkk".into(), vec![alias_value.clone()]),
        ("lll".into(), vec![alias_value.clone()]),
        ("mmm".into(), vec![alias_value.clone()]),
        ("nnn".into(), vec![alias_value.clone()]),
        ("ooo".into(), vec![alias_value.clone()]),
        ("ppp".into(), vec![alias_value.clone()]),
        ("qqq".into(), vec![alias_value.clone()]),
        ("rrr".into(), vec![alias_value.clone()]),
        ("sss".into(), vec![alias_value.clone()]),
        ("@".into(), vec![alias_value.clone()]),
        ("@@".into(), vec![alias_value.clone()]),
        ("@@@".into(), vec![alias_value]),
      ],
      ..ResolveOptions::default()
    },
  )
}

fn resolver_with_many_extensions() -> rspack_resolver::Resolver {
  Resolver::new(ResolveOptions {
    extensions: vec![
      ".bad0".to_string(),
      ".bad1".to_string(),
      ".bad2".to_string(),
      ".bad3".to_string(),
      ".bad4".to_string(),
      ".bad5".to_string(),
      ".bad6".to_string(),
      ".bad7".to_string(),
      ".bad8".to_string(),
      ".bad9".to_string(),
      ".mtsx".to_string(),
      ".mts".to_string(),
      ".mjs".to_string(),
      ".tsx".to_string(),
      ".ts".to_string(),
      ".jsx".to_string(),
      ".js".to_string(),
    ],
    imports_fields: vec![],
    exports_fields: vec![],
    enable_pnp: false,
    ..Default::default()
  })
}

fn create_async_resolve_task(
  rspack_resolver: Arc<rspack_resolver::Resolver>,
  path: PathBuf,
  request: String,
) -> impl Future<Output = ()> {
  async move {
    let _ = rspack_resolver.resolve(path, &request).await;
  }
}

fn bench_resolver(c: &mut Criterion) {
  let cwd = env::current_dir().unwrap().join("benches");

  let pkg_content = read_to_string("./benches/package.json").unwrap();
  let pkg_json: Value = serde_json::from_str(&pkg_content).unwrap();
  // about 1000 npm packages
  let data = pkg_json["dependencies"]
    .as_object()
    .unwrap()
    .keys()
    .map(|name| (&cwd, name))
    .collect::<Vec<_>>();

  // check validity
  runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        for (path, request) in &data {
            let r = rspack_resolver(false).resolve(path, request).await;
            if !r.is_ok() {
                panic!("resolve failed {path:?} {request},\n\nplease run `pnpm install --ignore-workspace` in `/benches` before running the benchmarks");
            }
        }
    });

  let symlink_test_dir = create_symlinks().expect("Create symlink fixtures failed");

  let symlinks_range = 0u32..10000;

  // check validity
  runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .unwrap()
    .block_on(async {
      for i in symlinks_range.clone() {
        assert!(
          rspack_resolver(false)
            .resolve(&symlink_test_dir, &format!("./file{i}"))
            .await
            .is_ok(),
          "file{i}.js"
        );
      }
    });

  let mut group = c.benchmark_group("resolver");

  // codspeed can only handle to up to 500 threads
  let multi_rt = || {
    Builder::new_multi_thread()
      .max_blocking_threads(256)
      .build()
      .expect("failed to create tokio runtime")
  };

  // force to use four threads
  rayon::ThreadPoolBuilder::new()
    .num_threads(4)
    .build_global()
    .expect("Failed to build global thread pool");

  group.bench_with_input(
    BenchmarkId::from_parameter("single-thread"),
    &data,
    |b, data| {
      let runner = runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

      let rspack_resolver = rspack_resolver(false);

      b.to_async(runner).iter_with_setup(
        || {
          rspack_resolver.clear_cache();
        },
        |_| async {
          for (path, request) in data {
            _ = rspack_resolver.resolve(path, request).await;
          }
        },
      );
    },
  );

  group.bench_with_input(
    BenchmarkId::from_parameter("[single-threaded]resolve with many extensions"),
    &data,
    |b, data| {
      let runner = runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");
      let rspack_resolver = resolver_with_many_extensions();

      b.to_async(runner).iter_with_setup(
        || {
          rspack_resolver.clear_cache();
        },
        |_| async {
          for (path, request) in data {
            _ = rspack_resolver
              .resolve(path, &format!("{}/bad", request))
              .await;
          }
        },
      );
    },
  );

  group.bench_with_input(
    BenchmarkId::from_parameter("multi-thread"),
    &data,
    |b, data| {
      let runner = multi_rt();
      let rspack_resolver = Arc::new(rspack_resolver(false));

      b.iter_with_setup(
        || {
          rspack_resolver.clear_cache();
        },
        |_| {
          runner.block_on(async {
            let mut join_set = JoinSet::new();
            data.iter().for_each(|(path, request)| {
              join_set.spawn(create_async_resolve_task(
                rspack_resolver.clone(),
                path.to_path_buf(),
                request.to_string(),
              ));
            });
            let _ = join_set.join_all().await;
          });
        },
      );
    },
  );

  group.bench_with_input(
    BenchmarkId::from_parameter("resolve from symlinks"),
    &symlinks_range,
    |b, data| {
      let runner = runtime::Runtime::new().expect("failed to create tokio runtime");
      let rspack_resolver = rspack_resolver(false);

      b.to_async(runner).iter_with_setup(
        || {
          rspack_resolver.clear_cache();
        },
        |_| async {
          for i in data.clone() {
            assert!(
              rspack_resolver
                .resolve(&symlink_test_dir, &format!("./file{i}"))
                .await
                .is_ok(),
              "file{i}.js"
            );
          }
        },
      );
    },
  );

  group.bench_with_input(
    BenchmarkId::from_parameter("resolve from symlinks multi thread"),
    &symlinks_range,
    |b, data| {
      let runner = multi_rt();
      let rspack_resolver = Arc::new(rspack_resolver(false));

      let symlink_test_dir = symlink_test_dir.clone();

      b.to_async(runner).iter(|| async {
        let mut join_set = JoinSet::new();

        data.clone().for_each(|i| {
          join_set.spawn(create_async_resolve_task(
            rspack_resolver.clone(),
            symlink_test_dir.clone(),
            format!("./file{i}").to_string(),
          ));
        });
        join_set.join_all().await;
      });
    },
  );

  let pnp_workspace = env::current_dir().unwrap().join("fixtures/pnp");
  let root_range = 1..11;

  group.bench_with_input(
    BenchmarkId::from_parameter("pnp resolve"),
    &root_range,
    |b, data| {
      let runner = runtime::Runtime::new().expect("failed to create tokio runtime");
      let rspack_resolver = Arc::new(rspack_resolver(true));

      b.iter_with_setup(
        || {
          // Drop all caches, then reload the PnP manifest before the timed
          // body runs. The manifest re-parse (~250KB regex compile) is
          // one-time work in real usage; keeping it out of the timed loop
          // lets resolver-level deltas surface.
          rspack_resolver.clear_cache();
          runner.block_on(async {
            let _ = rspack_resolver
              .resolve(pnp_workspace.join("1"), "preact")
              .await;
          });
        },
        |_| {
          runner.block_on(async {
            for i in data.clone() {
              let _ = rspack_resolver
                .resolve(pnp_workspace.join(format!("{i}")), "preact")
                .await;
            }
          });
        },
      );
    },
  );
}

// ============================================================================
// Specifier micro-benchmarks
// ----------------------------------------------------------------------------
// `parse_query_framgment` lives behind `Specifier::parse`. The wrapper does a
// single byte read + length check, so benchmarking `parse` is effectively
// benchmarking the query/fragment scanner. Cases are split into four groups:
// branch matrix, length sweep, escape scaling, and realistic specimens.
// ============================================================================

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
  resolver,
  bench_resolver,
  bench_specifier_branches,
  bench_specifier_length_sweep,
  bench_specifier_escape_scaling,
  bench_specifier_realistic
);
criterion_main!(resolver);
