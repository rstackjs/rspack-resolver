#![no_main]

use libfuzzer_sys::fuzz_target;
use rspack_resolver::{AliasValue, ResolveOptions, Resolver};

fuzz_target!(|data: &[u8]| {
    // First byte drives a few resolver-option toggles; the rest is the specifier.
    let Some((&cfg, rest)) = data.split_first() else {
        return;
    };
    let Ok(specifier) = std::str::from_utf8(rest) else {
        return;
    };
    if specifier.chars().any(char::is_control) {
        return;
    }

    let condition_names = if cfg & 0b001 == 0 {
        vec!["node".into(), "import".into()]
    } else {
        vec!["node".into(), "require".into()]
    };
    let extensions = if cfg & 0b010 == 0 {
        vec![".js".into(), ".json".into(), ".node".into()]
    } else {
        vec![".ts".into(), ".tsx".into(), ".js".into()]
    };
    let alias = if cfg & 0b100 == 0 {
        vec![]
    } else {
        vec![("@".into(), vec![AliasValue::Path("./src".into())])]
    };

    let resolver = Resolver::new(ResolveOptions {
        condition_names,
        extensions,
        alias,
        symlinks: cfg & 0b1000 == 0,
        ..ResolveOptions::default()
    });
    let cwd = std::env::current_dir().unwrap();
    // `resolve` is async; the future must be driven or the body is a no-op.
    let _ = futures::executor::block_on(resolver.resolve(cwd, specifier));
});
