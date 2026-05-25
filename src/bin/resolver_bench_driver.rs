use std::{env, fs::read_to_string, path::PathBuf, process};

use rspack_resolver::{AliasValue, FileSystemOptions, FileSystemOs, ResolveOptions, Resolver};
use serde_json::Value;

fn rspack_resolver(enable_pnp: bool) -> Resolver {
  #[cfg(not(feature = "yarn_pnp"))]
  let _ = enable_pnp;

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

fn resolver_with_many_extensions() -> Resolver {
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
    ..ResolveOptions::default()
  })
}

fn benchmark_requests() -> (PathBuf, Vec<String>) {
  let context = env::current_dir().unwrap().join("benches");
  let pkg_content = read_to_string("./benches/package.json").unwrap();
  let pkg_json: Value = serde_json::from_str(&pkg_content).unwrap();
  let requests = pkg_json["dependencies"]
    .as_object()
    .unwrap()
    .keys()
    .cloned()
    .collect::<Vec<_>>();
  (context, requests)
}

async fn run_resolve_dependencies(enable_pnp: bool) {
  let (context, requests) = benchmark_requests();
  let resolver = rspack_resolver(enable_pnp);

  for request in requests {
    let _ = resolver.resolve(&context, &request).await;
  }
}

async fn run_resolve_many_extensions() {
  let (context, requests) = benchmark_requests();
  let resolver = resolver_with_many_extensions();

  for request in requests.iter().take(200) {
    let _ = resolver.resolve(&context, request).await;
  }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
  let scenario = env::args().nth(1).unwrap_or_else(|| "deps".to_string());

  match scenario.as_str() {
    "deps" => run_resolve_dependencies(false).await,
    "many_extensions" => run_resolve_many_extensions().await,
    "pnp" => run_resolve_dependencies(true).await,
    _ => {
      eprintln!("unknown scenario: {scenario}. expected one of: deps, many_extensions, pnp");
      process::exit(2);
    }
  }
}
