///! See documentation at <https://docs.rs/rspack_resolver>
use std::{env, path::PathBuf, sync::Arc};

use rspack_resolver::{AliasValue, ResolveOptions, Resolver};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn init_tracing() {
  let trace_file = env::var("TRACE_FILE").ok();

  match trace_file {
    Some(file) => {
      // Use tracing-chrome to write Chrome-compatible trace directly
      let (chrome_layer, _guard) = tracing_chrome::ChromeLayerBuilder::new()
        .file(&file)
        .include_args(true)
        .build();

      tracing_subscriber::registry()
        .with(chrome_layer)
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::DEBUG.into()))
        .init();

      println!("Writing Chrome trace to: {}", file);
    }
    None => {
      tracing_subscriber::registry()
        .with(fmt::layer().with_span_events(fmt::format::FmtSpan::CLOSE))
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::DEBUG.into()))
        .init();
    }
  }
}

#[tokio::main]
async fn main() {
  init_tracing();

  let path = PathBuf::from(env::args().nth(1).expect("path"));

  assert!(
    path.is_dir(),
    "{path:?} must be a directory that will be resolved against."
  );
  assert!(path.is_absolute(), "{path:?} must be an absolute path.",);

  let specifier = env::args().nth(2).expect("specifier");

  println!("path: {path:?}");
  println!("specifier: {specifier}");

  let options = ResolveOptions {
    alias_fields: vec![vec!["browser".into()]],
    alias: vec![("asdf".into(), vec![AliasValue::from("./test.js")])],
    extensions: vec![".js".into(), ".ts".into()],
    extension_alias: vec![(".js".into(), vec![".ts".into(), ".js".into()])],
    condition_names: vec!["node".into(), "import".into()],
    ..ResolveOptions::default()
  };
  let mut ctx = Default::default();

  match Resolver::new(options)
    .resolve_with_context(path, &specifier, &mut ctx)
    .await
  {
    Err(error) => println!("Error: {error}"),
    Ok(resolution) => println!("Resolved: {:?}", resolution.full_path()),
  };

  let mut sorted_file_deps = ctx.file_dependencies.iter().collect::<Vec<_>>();
  sorted_file_deps.sort();
  println!("file_deps: {:#?}", sorted_file_deps);

  let mut sorted_missing = ctx.missing_dependencies.iter().collect::<Vec<_>>();
  sorted_missing.sort();
  println!("missing_deps: {:#?}", sorted_missing);
}
