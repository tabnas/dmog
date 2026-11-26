// Shared helpers of the tests moved here from tabnas-alchemy: the tests
// that lower and run a program need real routers and renderers, which
// alchemy does not depend on, and this crate, the composition root, does.
// Cargo compiles this module into EVERY integration test binary, so an
// item only one binary uses is dead code in the others; the allow keeps
// that from being a warning rather than hiding anything real.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tabnas_alchemy::shared::{Fail, Flow, Limits, Metrics, Renderers, Routers, Sink};
use tabnas_alchemy::value::Val;
use tabnas_alchemy::{Input, Program, Source};
use tabnas_dmog::translate;
use tabnas_support::Value;

/// The routers a host passes alchemy: transduce's.
pub fn routers() -> Arc<dyn Routers<Val>> {
    Arc::new(tabnas_transduce::routers())
}

/// The renderers a host passes alchemy: render's.
pub fn renderers() -> Arc<dyn Renderers> {
    Arc::new(tabnas_render::renderers())
}

/// [`tabnas_alchemy::compile`] with transduce's routers and render's
/// renderers, as a host compiles.
pub fn compile(src: &str, file: &str) -> Result<Program, Fail> {
    tabnas_alchemy::compile(src, file, routers(), renderers())
}

/// [`tabnas_alchemy::compile_with`] with transduce's routers and render's
/// renderers: a program over the input the host declares.
pub fn compile_with(src: &str, file: &str, input: Input) -> Result<Program, Fail> {
    tabnas_alchemy::compile_with(src, file, input, routers(), renderers())
}

/// [`tabnas_alchemy::compile_sources`] with transduce's routers and
/// render's renderers.
pub fn compile_sources(sources: &[Source<'_>]) -> Result<Program, Fail> {
    tabnas_alchemy::compile_sources(sources, routers(), renderers())
}

/// Push the events of the JSON document `text` into `sink`, as `alchemy
/// run` reads its input: JSON's own transducer part over the parser's
/// rule passes, the engine's arrays emptied behind it. The run's outcome,
/// a failure of the sink's as it was.
pub fn json_into(
    text: &str,
    limits: &Limits,
    metrics: Arc<Metrics>,
    sink: Box<dyn Sink + Send>,
) -> Result<Flow, Fail> {
    let json = translate::format("json").expect("the command reads JSON");
    let part = json
        .transducer_program()?
        .expect("the JSON grammar ships a transducer part");
    let head = part.parser_sink_into(sink, limits, metrics.clone())?;
    json.passes(text)?
        .grammar(json.id())
        .prune()
        .limits(limits.clone())
        .metrics(metrics)
        .run_boxed(head)
        .0
}

/// This repository's root: the parent of `rs/`.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("rs/ has a parent")
        .to_path_buf()
}

/// alchemy's test files, vendored into `test/fixtures/alchemy` by
/// `tools/vendor-fixtures.sh`; the other repositories' are beside it, each
/// under its own name. Missing files fail, naming where they were looked
/// for.
pub fn alchemy_root() -> PathBuf {
    let root = vendored().join("alchemy");
    assert!(
        root.join("test/spec").is_dir(),
        "no vendored alchemy test/spec at {}: run tools/vendor-fixtures.sh",
        root.display()
    );
    root
}

/// The test files vendored from other tabnas repositories, each at its
/// path there: `test/fixtures`, or `DMOG_FIXTURES`, which the gates set to
/// the directory of the sibling checkouts when they run against them
/// (`DMOG_SIBLINGS=1`).
pub fn vendored() -> PathBuf {
    match std::env::var_os("DMOG_FIXTURES") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => repo_root().join("test/fixtures"),
    }
}

/// alchemy's shared `test/spec` directory, vendored.
pub fn spec_dir() -> PathBuf {
    alchemy_root().join("test/spec")
}

/// The code a `Fail` from alchemy pins: the reader and the desugarer
/// both lead the message with it (`bad_dedent: ...`, `empty_step: ...`),
/// so `ERROR:<code>` in a fixture names that word. A message without the
/// convention pins nothing but the transduce code.
pub fn fail_code(fail: &Fail) -> String {
    fail.message.split_once(": ").map_or_else(
        || fail.code.as_str().to_string(),
        |(code, _)| code.to_string(),
    )
}

/// Canonical text as the fixture data model: the expected column is a
/// JSON string of it.
pub fn text_value(text: String) -> Value {
    Value::String(text)
}
