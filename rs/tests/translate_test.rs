//! The cross product `dmog translate` answers for: every document of
//! the corpora, read with its format's grammar, written in every format
//! the command carries, read back with that format's grammar, and compared
//! with the document's own value under the target's declared conventions
//! (its loss list). A pair that fails to write, that writes a document its
//! own grammar refuses, or that reads back as anything but the conventions
//! say is a failure, and so is a corpus that shrinks.
//!
//! A pair whose target declares that it refuses the document is held to
//! that refusal, its code and the start of its message, and counted: a
//! schema-only target (one that writes a schema's tree with no embedding
//! into it: C, CSS, PGN, proto and the grammar notations ABNF, EBNF and
//! GBNF) refuses another format's tree, a grammar notation refuses a
//! grammar spec it has no form for, naming what it met, and Semantic
//! Versioning's embedding refuses a tree that is not a version. A refusal
//! of another kind, or a document written where a refusal is declared, is
//! a failure. The grammar notations share a schema, the grammar spec their
//! compilers emit, so each writes the others' documents.
//!
//! The corpora are the sibling checkouts': transduce's fixtures (aless's: a
//! document of every format but C, CSS, expressions, PGN, proto, Semantic
//! Versioning and the grammar notations, whose own fixtures the release
//! run reads, and more for YAML and ZON), the documents of JSONTestSuite
//! every JSON parser must accept (jsonc's conformance pins), and the
//! example grammars of the grammar notations' repositories. A fixture its
//! own grammar refuses (ZON's repeated fields) is no document, and is
//! counted as one refused; so is a document of a format whose reader is a
//! registered defect (`READER_DEFECTS`), counted apart.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tabnas_alchemy::translate::Options;
use tabnas_dmog::translate::{self, Document, Format, Request};
use tabnas_transduce::{Code, Datum, Fail, Limits, Metrics};

/// What a run wrote, shared with the test.
#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);

impl Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The directory the sibling checkouts are in.
fn siblings() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The format a fixture's extension names, by its id.
fn format_of(extension: &str) -> Option<&'static str> {
    Some(match extension {
        "json" => "json",
        "json5" => "json5",
        "jsonc" => "jsonc",
        "jsonic" => "jsonic",
        "jsonl" => "jsonl",
        "csv" => "csv",
        "md" => "markdown",
        "toml" => "toml",
        "ini" => "ini",
        "xml" => "xml",
        "yaml" => "yaml",
        "zon" => "zon",
        "rss" | "atom" => "feed",
        "c" | "h" => "c",
        "abnf" => "abnf",
        "ebnf" => "ebnf",
        "gbnf" => "gbnf",
        _ => return None,
    })
}

/// Every document of the corpora: its name, its format and its text.
fn corpus() -> Vec<(String, &'static str, String)> {
    let mut docs = Vec::new();
    read_corpora(
        &mut docs,
        &[
            (
                "transduce",
                siblings().join("transduce/rs/tests/fixtures"),
                None,
                None,
            ),
            (
                "JSONTestSuite",
                siblings().join("jsonc/test/JSONTestSuite/test_parsing"),
                Some("y_"),
                None,
            ),
        ],
    );
    docs
}

/// A corpus: its name, its directory, the prefix its documents' file names
/// start with, where it is held to one, and the one format it is held to,
/// where it is (a grammar notation's examples sit beside a README).
type Corpus = (
    &'static str,
    PathBuf,
    Option<&'static str>,
    Option<&'static str>,
);

/// The documents of `dirs`, in name order, each as its extension's format
/// reads it: a file of no format here, or one that is not UTF-8, is none.
fn read_corpora(docs: &mut Vec<(String, &'static str, String)>, dirs: &[Corpus]) {
    for (corpus, dir, prefix, only) in dirs {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("{corpus}: cannot read {}: {e}", dir.display()))
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if prefix.is_some_and(|p| !name.starts_with(p)) {
                continue;
            }
            let Some(id) = path
                .extension()
                .and_then(|e| e.to_str())
                .and_then(format_of)
            else {
                continue;
            };
            if only.is_some_and(|only| only != id) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            docs.push((format!("{corpus}/{name}"), id, text));
        }
    }
}

/// The repositories whose fixture corpora the matrix reads, each with the
/// id its format's manifest gives it (the repository's name, but for
/// tabnas/chess, whose format is PGN) and the column its rows hold a
/// document of the format in: the input, but for ABNF, whose rows hold a
/// grammar and an input to that grammar. EBNF's and GBNF's repositories
/// keep no fixtures of that kind; their example grammars, and ABNF's, are
/// files (`NOTATION_EXAMPLES`).
const SPEC_CORPORA: [(&str, &str, &str); 20] = [
    ("abnf", "abnf", "grammar"),
    ("c", "c", "input"),
    ("chess", "pgn", "input"),
    ("css", "css", "input"),
    ("csv", "csv", "input"),
    ("expr", "expr", "input"),
    ("feed", "feed", "input"),
    ("ini", "ini", "input"),
    ("json", "json", "input"),
    ("json5", "json5", "input"),
    ("jsonc", "jsonc", "input"),
    ("jsonic", "jsonic", "input"),
    ("jsonl", "jsonl", "input"),
    ("markdown", "markdown", "input"),
    ("proto", "proto", "input"),
    ("semver", "semver", "input"),
    ("toml", "toml", "input"),
    ("xml", "xml", "input"),
    ("yaml", "yaml", "input"),
    ("zon", "zon", "input"),
];

/// The grammar notations' example grammars, which their repositories keep
/// as files: each notation's directory, below its repository's checkout.
const NOTATION_EXAMPLES: [(&str, &str); 3] = [
    ("abnf", "abnf/ts/test/grammar"),
    ("ebnf", "ebnf/ts/test/grammar"),
    ("gbnf", "gbnf/test/corpus"),
];

/// Every format's own fixture corpus: the document of every row of its
/// repository's `test/spec/*.tsv`, decoded as the fixture runner decodes
/// it, once each, and the grammar notations' example grammars. A row that
/// sets options of its own (`opts`) is read by another reader than the
/// format's default, and an error row's document is one the format refuses,
/// so neither is a document of the format here. Where a row holds a grammar
/// and an input to it (ABNF's), every input a grammar's rows give it, those
/// it accepts and those it refuses, is a sample of that grammar's, added to
/// `samples` under the name of its document.
fn spec_corpus(
    docs: &mut Vec<(String, &'static str, String)>,
    samples: &mut HashMap<String, Vec<String>>,
) {
    let examples: Vec<Corpus> = NOTATION_EXAMPLES
        .iter()
        .map(|(id, dir)| (*id, siblings().join(dir), None, Some(*id)))
        .collect();
    for (repository, id, column) in SPEC_CORPORA {
        let dir = siblings().join(repository).join("test/spec");
        let files = tabnas_support::load_spec_dir(&dir, &tabnas_support::SpecOptions::default())
            .unwrap_or_else(|e| panic!("{id}: cannot read {}: {e}", dir.display()));
        let mut seen = HashSet::new();
        let mut inputs: HashMap<String, Vec<String>> = HashMap::new();
        let first = docs.len();
        for file in files {
            if !file.header.iter().any(|name| name == column) {
                continue;
            }
            let given = column != "input" && file.header.iter().any(|name| name == "input");
            for row in &file.rows {
                if !row.named("opts").trim().is_empty() {
                    continue;
                }
                let document = row.unesc_named(column);
                if given {
                    let sample = row.unesc_named("input");
                    let known = inputs.entry(document.clone()).or_default();
                    if !known.contains(&sample) {
                        known.push(sample);
                    }
                }
                if tabnas_support::is_error_expect(row.named("expected")) {
                    continue;
                }
                if seen.insert(document.clone()) {
                    docs.push((
                        format!("{repository}/{}:{}", file.file, row.line),
                        id,
                        document,
                    ));
                }
            }
        }
        for (name, _, text) in &docs[first..] {
            if let Some(given) = inputs.get(text) {
                samples.insert(name.clone(), given.clone());
            }
        }
    }
    read_corpora(docs, &examples);
}

/// What `test/notation-samples.json` holds: the inputs this repository
/// gives the grammar notations' example grammars, the ones their
/// repositories' own tests give them, by the name the cross product gives a
/// document (`samples`), and the pairs that recognise some of their samples
/// otherwise across the lexing, each with exactly those samples, by the
/// name the cross product gives a pair (`otherwise`).
struct Notation {
    samples: HashMap<String, Vec<String>>,
    otherwise: HashMap<String, Vec<String>>,
}

fn notation_samples() -> Notation {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test/notation-samples.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let file: serde_json::Value = serde_json::from_str(&text).expect("the samples are JSON");
    let lists = |key: &str| -> HashMap<String, Vec<String>> {
        file[key]
            .as_object()
            .unwrap_or_else(|| panic!("{key} is an object"))
            .iter()
            .map(|(name, list)| {
                let list = list
                    .as_array()
                    .unwrap_or_else(|| panic!("{key}: {name} is a list"))
                    .iter()
                    .map(|sample| {
                        sample
                            .as_str()
                            .unwrap_or_else(|| {
                                panic!("{key}: {name} holds a sample that is no string")
                            })
                            .to_string()
                    })
                    .collect();
                (name.clone(), list)
            })
            .collect()
    };
    Notation {
        samples: lists("samples"),
        otherwise: lists("otherwise"),
    }
}

fn format(id: &str) -> &'static Format {
    translate::format(id).unwrap_or_else(|| panic!("{id} is a format"))
}

// ---------------------------------------------------------------------
// The readers registered as defective
// ---------------------------------------------------------------------

/// The formats whose reader, as this command reads a document through
/// transduce, does not build the tree the format's parts declare, for a
/// defect of the format's package: each id with its defect. The matrix
/// reads no document of a registered format as a source, and counts the
/// documents it leaves out; every format is still a target.
/// `a_registered_reader_defect_still_stands` holds each entry to an example
/// document, comparing what the command reads with what the package's own
/// API builds of it, so an entry fails once its reader is repaired, and
/// must then be deleted. None is registered: proto's, the last, was
/// repaired when tabnas-proto gave the descriptor as a tree
/// (`tabnas_proto::parse_value`), and
/// `a_proto_file_reads_as_the_descriptor_its_package_builds` holds it so.
const READER_DEFECTS: [(&str, &str); 0] = [];

/// Whether documents of `id` are left out as sources for a registered
/// reader defect.
fn registered_defect(id: &str) -> bool {
    READER_DEFECTS.iter().any(|(defective, _)| *defective == id)
}

/// A value as serde_json holds it, as a `Datum`: what a package's own API
/// builds, for the comparisons here.
fn datum_of_json(j: &serde_json::Value) -> Datum {
    use serde_json::Value as J;
    match j {
        J::Null => Datum::Null,
        J::Bool(b) => Datum::Bool(*b),
        J::Number(n) => Datum::Number {
            value: n.as_f64().unwrap_or(f64::NAN),
            lexeme: None,
        },
        J::String(s) => Datum::String(s.as_str().into()),
        J::Array(items) => Datum::Array(items.iter().map(datum_of_json).collect()),
        J::Object(members) => Datum::Object(
            members
                .iter()
                .map(|(k, v)| (k.as_str().into(), datum_of_json(v)))
                .collect(),
        ),
    }
}

/// Each registered reader defect still stands: the tree this command reads
/// an example document as is not the one the format's package builds with
/// its own API, the tree its parts declare. Once a reader is repaired this
/// fails, until its entry in `READER_DEFECTS` is deleted.
#[test]
fn a_registered_reader_defect_still_stands() {
    let limits = Limits::default();
    for (id, defect) in READER_DEFECTS {
        let (example, declared) = defect_example(id);
        let read = format(id).read(example, &limits).unwrap();
        assert!(
            !same(&read, &declared),
            "{id}'s reader now reads {example:?} as the tree its package builds, {declared}: \
             delete its entry ({defect})"
        );
    }
}

/// A registered defect's example document, with the tree the format's
/// package builds of it with its own API. None is registered, so an entry
/// fails here until its example is added.
fn defect_example(id: &str) -> (&'static str, Datum) {
    panic!("{id}: a registered defect needs an example here")
}

/// A .proto file reads as the FileDescriptorProto tabnas-proto's parse
/// builds of it, the tree proto's parts declare, with its members in the
/// order the canonical parse gives them, and so is written back by
/// proto's render, where its reader once walked the syntax tree, which the
/// render refused.
#[test]
fn a_proto_file_reads_as_the_descriptor_its_package_builds() {
    let text =
        "syntax = \"proto3\";\npackage p;\nmessage M { int32 a = 1; optional string b = 2; }\n";
    let read = format("proto").read(text, &Limits::default()).unwrap();
    let descriptor = tabnas_proto::parse(text, None).unwrap();
    let declared = datum_of_json(&serde_json::to_value(&descriptor).unwrap());
    assert!(same(&read, &declared), "{read} is not {declared}");
    let tree = Datum::from_tabnas(&tabnas_proto::parse_value(text, None).unwrap());
    assert_eq!(read.to_string(), tree.to_string());
    let proto = format("proto");
    assert_eq!(
        translate_text(proto, proto, text, None).unwrap(),
        "syntax = \"proto3\";\npackage p;\nmessage M {\n  int32 a = 1;\n  optional string b = 2;\n}\n"
    );
}

// ---------------------------------------------------------------------
// The refusals the targets declare
// ---------------------------------------------------------------------

/// What a pair is held to: written, and read back under the target's
/// conventions; refused as the target declares, with the code and the
/// start of the message alchemy's composition or the target's part gives;
/// or written unless the target refuses it so, where the target declares
/// that it refuses what it has no form for, naming what it met.
enum Expect {
    Written,
    Refused(Code, String),
    WrittenUnlessRefused(Code, String),
}

/// Whether a format is a grammar notation: one whose documents read as
/// the grammar spec the tabnas BNF compiler emits, which each notation's
/// render writes back.
fn grammar_notation(format: &Format) -> bool {
    format.part.schema.as_deref() == Some("grammar-spec")
}

/// What `from`'s document, read as `source`, into `to` is held to. A
/// schema-only target (one that writes from a tree, with a schema and no
/// embed: C, CSS, PGN, proto and the grammar notations) refuses a tree of
/// another schema before any output, as alchemy's composition declares; a
/// grammar notation's render writes a grammar spec, any notation's, unless
/// it has no form for something in it, which it refuses naming what it
/// met, as its loss list declares (an action, a negated class in ABNF, the
/// engine's own tokens in GBNF, ...); Semantic Versioning's embedding
/// refuses a tree that is not a version, as its part declares; and expr's
/// render refuses a list led by an object whose `src` its reader would
/// take for the list's operator, where that is no default operator's
/// source text (a C syntax tree's token), as its loss list declares
/// ([`expr_refuses`]). Every other pair is written, Markdown's table among
/// them: it writes from records, which any tree makes.
fn expect(from: &Format, to: &Format, source: &Datum) -> Expect {
    let target = &to.part;
    let foreign = target.writes == tabnas_alchemy::translate::Shape::Tree
        && target.schema.is_some()
        && from.part.schema != target.schema;
    match (&target.schema, &target.embed) {
        (Some(schema), None) if foreign => Expect::Refused(
            Code::TargetValueUnrepresentable,
            format!("schema_only: {} writes a {schema} tree, ", target.id),
        ),
        _ if foreign && to.id() == "semver" && !semver_version(source) => Expect::Refused(
            Code::TargetValueUnrepresentable,
            "the document is not a version: ".to_string(),
        ),
        _ if to.id() == "expr" && expr_refuses(source) => Expect::Refused(
            Code::TargetValueUnrepresentable,
            "the object at ".to_string(),
        ),
        _ if grammar_notation(to) => Expect::WrittenUnlessRefused(
            Code::TargetValueUnrepresentable,
            format!(
                "the grammar spec cannot be written as {}: ",
                to.id().to_uppercase()
            ),
        ),
        _ => Expect::Written,
    }
}

/// The text a number is written with when it is digits alone: its lexeme,
/// or the text JSON writes for it, ECMAScript's, which spells a whole
/// number below 10^21 with its digits.
fn written_digits(d: &Datum) -> Option<String> {
    let Datum::Number { value, lexeme } = d else {
        return None;
    };
    let text = match lexeme {
        Some(lexeme) => lexeme.to_string(),
        None if value.is_finite()
            && value.fract() == 0.0
            && !value.is_sign_negative()
            && *value < 1e21 =>
        {
            format!("{}", *value as u128)
        }
        None => return None,
    };
    (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())).then_some(text)
}

/// Whether a string is digits with no leading zero, but for `0` itself.
fn digit_string(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) && (s == "0" || !s.starts_with('0'))
}

/// Whether a value is a prerelease (`prerelease`) or build identifier: a
/// number written in digits, or a string of 0-9, A-Z, a-z and -, not
/// empty, which for a prerelease is no number of digits that begins with a
/// zero.
fn semver_identifier(d: &Datum, prerelease: bool) -> bool {
    match d {
        Datum::Number { .. } => written_digits(d).is_some(),
        Datum::String(s) => {
            !s.is_empty()
                && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && !(prerelease && s.bytes().all(|b| b.is_ascii_digit()) && !digit_string(s))
        }
        _ => false,
    }
}

/// Whether a value is a version as Semantic Versioning's embedding takes
/// one (its part's `alchemy/embed.alc`): an object whose major, minor and
/// patch are whole numbers written in digits (a number whose text is digits
/// alone, or a string of digits with no leading zero), and whose prerelease
/// and build, where it has them, are null, the empty string, or a list of
/// identifiers or one string of them joined by dots.
fn semver_version(d: &Datum) -> bool {
    let Some(m) = d.as_object() else {
        return false;
    };
    let core = |key: &str| match m.get(key) {
        Some(n @ Datum::Number { .. }) => written_digits(n).is_some(),
        Some(Datum::String(s)) => digit_string(s),
        _ => false,
    };
    let identifiers = |key: &str, prerelease: bool| match m.get(key) {
        None | Some(Datum::Null) => true,
        Some(Datum::String(s)) => {
            s.is_empty()
                || s.split('.')
                    .all(|id| semver_identifier(&Datum::String(id.into()), prerelease))
        }
        Some(Datum::Array(items)) => items.iter().all(|i| semver_identifier(i, prerelease)),
        Some(_) => false,
    };
    core("major")
        && core("minor")
        && core("patch")
        && identifiers("prerelease", true)
        && identifiers("build", false)
}

/// A pair's source, target and program, by id and text.
type PairKey = (String, String, Option<String>, bool);

/// A pair's composition, compiled.
type Compiled = Rc<translate::Compiled>;

thread_local! {
    /// The compositions compiled so far, by source, target and program:
    /// one compiled composition serves every document of a pair.
    static COMPILED: RefCell<HashMap<PairKey, Compiled>> = RefCell::new(HashMap::new());
}

/// Translate `text`, read as `from`, into `to`, with `program` in front
/// when there is one.
fn translate_text(
    from: &Format,
    to: &Format,
    text: &str,
    program: Option<(&str, &str)>,
) -> Result<String, Fail> {
    translate_document(from, to, Document::Text(text), program, false)
}

/// [`translate_text`], read whole, as `--whole` reads it.
fn translate_whole(from: &Format, to: &Format, text: &str) -> Result<String, Fail> {
    translate_document(from, to, Document::Text(text), None, true)
}

/// Whether a translation failed because the source's transducer part cannot
/// stream the document (a repeated member, a stream of several YAML
/// documents, two plain `<<` in one YAML mapping, a YAML merge source that
/// repeats a merged key): its code, and the route that reads it named,
/// since the output streams and is never written twice (admin ADR-33).
fn unstreamed(fail: &Fail) -> bool {
    matches!(
        fail.code,
        Code::StreamabilityUnknown | Code::DuplicateMember
    ) && fail.message.ends_with("; --whole reads the document whole")
}

/// [`translate_text`] over a document, which may be a file, read whole
/// where `whole` is asked.
fn translate_document(
    from: &Format,
    to: &Format,
    document: Document<'_>,
    program: Option<(&str, &str)>,
    whole: bool,
) -> Result<String, Fail> {
    let request = Request {
        from,
        to,
        path: None,
        options: Options::default(),
        program,
        whole,
        limits: Limits::default(),
    };
    let key = (
        from.id().to_string(),
        to.id().to_string(),
        program.map(|(_, text)| text.to_string()),
        whole,
    );
    let compiled = match COMPILED.with(|c| c.borrow().get(&key).cloned()) {
        Some(compiled) => compiled,
        None => {
            let compiled = Rc::new(translate::compile(&request)?);
            COMPILED.with(|c| c.borrow_mut().insert(key, compiled.clone()));
            compiled
        }
    };
    let out = Buffer::default();
    translate::run_document(
        &request,
        &compiled,
        document,
        Box::new(out.clone()),
        Metrics::new(),
    )?;
    let bytes = out.0.lock().unwrap().clone();
    Ok(String::from_utf8(bytes).expect("a translation writes UTF-8"))
}

// ---------------------------------------------------------------------
// Values compared as the conventions compare them
// ---------------------------------------------------------------------

/// Two values the same: numbers by value (NaN is NaN), objects by their
/// members whatever their order, arrays in order.
fn same(a: &Datum, b: &Datum) -> bool {
    match (a, b) {
        (Datum::Number { value: x, .. }, Datum::Number { value: y, .. }) => {
            x == y || (x.is_nan() && y.is_nan())
        }
        (Datum::Array(x), Datum::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        (Datum::Object(x), Datum::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        (a, b) => a == b,
    }
}

/// The value with every number that is not finite replaced.
fn map_non_finite(d: &Datum, f: &dyn Fn(f64) -> Datum) -> Datum {
    match d {
        Datum::Number { value, .. } if !value.is_finite() => f(*value),
        Datum::Array(items) => Datum::Array(items.iter().map(|i| map_non_finite(i, f)).collect()),
        Datum::Object(members) => Datum::Object(
            members
                .iter()
                .map(|(k, v)| (k.clone(), map_non_finite(v, f)))
                .collect(),
        ),
        d => d.clone(),
    }
}

fn wrap_object(d: &Datum, key: &str) -> Datum {
    match d {
        Datum::Object(_) => d.clone(),
        d => Datum::Object([(key.into(), d.clone())].into_iter().collect()),
    }
}

fn wrap_array(d: &Datum) -> Datum {
    match d {
        Datum::Array(_) => d.clone(),
        d => Datum::Array(vec![d.clone()]),
    }
}

/// TOML's conventions: no null (a member whose value is null is not
/// written, a null element is skipped).
fn without_nulls(d: &Datum) -> Datum {
    match d {
        Datum::Array(items) => Datum::Array(
            items
                .iter()
                .filter(|i| !matches!(i, Datum::Null))
                .map(without_nulls)
                .collect(),
        ),
        Datum::Object(members) => Datum::Object(
            members
                .iter()
                .filter(|(_, v)| !matches!(v, Datum::Null))
                .map(|(k, v)| (k.clone(), without_nulls(v)))
                .collect(),
        ),
        d => d.clone(),
    }
}

/// The text a cell is written as in CSV and Markdown: a string as it is,
/// a number by its value (compared as one), a non-finite one by its word,
/// a boolean by its name, null and an absent member as the empty field, a
/// container as its compact JSON text.
enum Cell {
    Text(String),
    Number(f64),
}

fn cell(d: Option<&Datum>) -> Cell {
    match d {
        None | Some(Datum::Null) => Cell::Text(String::new()),
        Some(Datum::Bool(b)) => Cell::Text(b.to_string()),
        Some(Datum::Number { value, .. }) if value.is_nan() => Cell::Text("NaN".into()),
        Some(Datum::Number { value, .. }) if value.is_infinite() => Cell::Text(
            if *value > 0.0 {
                "Infinity"
            } else {
                "-Infinity"
            }
            .into(),
        ),
        Some(Datum::Number { value, .. }) => Cell::Number(*value),
        Some(Datum::String(s)) => Cell::Text(s.to_string()),
        Some(d) => Cell::Text(d.to_string()),
    }
}

fn cell_is(expected: &Cell, got: &str) -> bool {
    match expected {
        Cell::Text(t) => t == got,
        Cell::Number(n) => got.parse::<f64>().is_ok_and(|g| g == *n),
    }
}

/// The table the inferred binding makes of a value: the rows are the
/// root array's elements (a root of another kind is one row), the columns
/// the first row's (an object's keys, an array's positions, or one
/// `value` column for a scalar), each row's cell found by the column's
/// path.
fn table(d: &Datum) -> (Vec<String>, Vec<Vec<Cell>>) {
    let rows = match wrap_array(d) {
        Datum::Array(rows) => rows,
        _ => unreachable!(),
    };
    enum Path {
        Key(String),
        Index(usize),
        Itself,
    }
    let columns: Vec<(String, Path)> = match rows.first() {
        None => Vec::new(),
        Some(Datum::Object(m)) => m
            .keys()
            .map(|k| (k.to_string(), Path::Key(k.to_string())))
            .collect(),
        Some(Datum::Array(items)) => (0..items.len())
            .map(|i| (i.to_string(), Path::Index(i)))
            .collect(),
        Some(_) => vec![("value".to_string(), Path::Itself)],
    };
    let cells = rows
        .iter()
        .map(|row| {
            columns
                .iter()
                .map(|(_, path)| match path {
                    Path::Key(k) => cell(row.as_object().and_then(|m| m.get(k.as_str()))),
                    Path::Index(i) => cell(row.as_array().and_then(|a| a.get(*i))),
                    Path::Itself => cell(Some(row)),
                })
                .collect()
        })
        .collect();
    (columns.into_iter().map(|(l, _)| l).collect(), cells)
}

/// Whether a read-back table (an array of objects keyed by label, every
/// value a string) is the table the inferred binding makes of `source`,
/// with each cell's text passed through `cell_text` first (Markdown's
/// normalisation of what it writes).
fn check_records(
    source: &Datum,
    back: &Datum,
    cell_text: &dyn Fn(&str) -> String,
) -> Result<(), String> {
    let (labels, rows) = table(source);
    let back_rows = back
        .as_array()
        .ok_or_else(|| format!("read back as {back}, not an array of records"))?;
    if labels.is_empty() {
        return if back_rows.is_empty() {
            Ok(())
        } else {
            Err(format!("a table of no columns read back as {back}"))
        };
    }
    if back_rows.len() != rows.len() {
        return Err(format!(
            "{} rows read back, {} written: {back}",
            back_rows.len(),
            rows.len()
        ));
    }
    for (i, (row, got)) in rows.iter().zip(back_rows).enumerate() {
        let got = got
            .as_object()
            .ok_or_else(|| format!("row {i} read back as {got}"))?;
        for (label, expected) in labels.iter().zip(row) {
            // A label is a header cell, written and read back as any cell.
            let text = match got.get(cell_text(label).as_str()) {
                Some(Datum::String(s)) => s.to_string(),
                Some(Datum::Null) | None => String::new(),
                Some(other) => other.to_string(),
            };
            let expected = match expected {
                Cell::Text(t) => Cell::Text(cell_text(t)),
                Cell::Number(n) => Cell::Number(*n),
            };
            if !cell_is(&expected, &text) {
                return Err(format!(
                    "row {i}, column {label:?}: read back {text:?}, where {} was written",
                    match expected {
                        Cell::Text(t) => format!("{t:?}"),
                        Cell::Number(n) => n.to_string(),
                    }
                ));
            }
        }
    }
    Ok(())
}

/// Markdown's normalisation of a written cell: a line break is a space,
/// a U+0000 is U+FFFD, and the whitespace at either end is not kept, as
/// the reader trims it: what JavaScript's `trim` takes, which is Unicode's
/// whitespace without U+0085 and with U+FEFF.
fn markdown_cell(text: &str) -> String {
    text.replace("\r\n", " ")
        .replace(['\n', '\r'], " ")
        .replace('\0', "\u{fffd}")
        .trim_matches(|c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}')
        .to_string()
}

// ---------------------------------------------------------------------
// The cross product
// ---------------------------------------------------------------------

/// Whether an INI document read back (`back`) is what INI's conventions
/// make of `expected`: an object is a section (or the root) and an array
/// of scalars is `key[]` lines, each read back as itself; a number reads
/// back as its text, and one that is not finite as its word; a container
/// INI has no place for (inside an array, an empty array, or under a key
/// no header can spell) reads back as its compact JSON text, a string;
/// true, false and null read back as themselves, and a string as itself.
fn ini_same(expected: &Datum, back: &Datum) -> bool {
    match (expected, back) {
        (Datum::Object(e), Datum::Object(b)) => {
            e.len() == b.len()
                && e.iter().all(|(k, v)| {
                    b.get(k)
                        .or_else(|| b.get(k.trim()))
                        .is_some_and(|w| ini_same(v, w))
                })
        }
        (Datum::Array(e), Datum::Array(b)) if !e.is_empty() => {
            e.len() == b.len() && e.iter().zip(b).all(|(x, y)| ini_item(x, y))
        }
        (Datum::Number { value, .. }, Datum::String(s)) => number_text_is(*value, s),
        (container @ (Datum::Object(_) | Datum::Array(_)), Datum::String(s)) => {
            json_text_is(container, s)
        }
        (e, b) => same(e, b),
    }
}

/// An array item: a scalar as `ini_same` reads it, a container as its
/// JSON text.
fn ini_item(expected: &Datum, back: &Datum) -> bool {
    match (expected, back) {
        (container @ (Datum::Object(_) | Datum::Array(_)), Datum::String(s)) => {
            json_text_is(container, s)
        }
        (e, b) => ini_same(e, b),
    }
}

/// Whether `text` spells the number `value`: its digits, or the word of
/// one that is not finite.
fn number_text_is(value: f64, text: &str) -> bool {
    match text {
        "Infinity" => value == f64::INFINITY,
        "-Infinity" => value == f64::NEG_INFINITY,
        "NaN" => value.is_nan(),
        t => t.parse::<f64>().is_ok_and(|n| n == value),
    }
}

/// Whether `text` is the compact JSON text of `container`, read back as
/// JSON and compared as values, a number that is not finite matching
/// null or its word.
fn json_text_is(container: &Datum, text: &str) -> bool {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
        return false;
    };
    fn matches(d: &Datum, j: &serde_json::Value) -> bool {
        use serde_json::Value as J;
        match (d, j) {
            (Datum::Null, J::Null) | (Datum::Number { .. }, J::Null) => true,
            (Datum::Bool(a), J::Bool(b)) => a == b,
            (Datum::Number { value, .. }, J::Number(n)) => n.as_f64() == Some(*value),
            (Datum::Number { value, .. }, J::String(s)) => number_text_is(*value, s),
            (Datum::String(a), J::String(b)) => **a == **b,
            (Datum::Array(a), J::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(x, y)| matches(x, y))
            }
            (Datum::Object(a), J::Object(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .all(|(k, v)| b.get(&**k).is_some_and(|w| matches(v, w)))
            }
            _ => false,
        }
    }
    matches(container, &json)
}

/// Whether a string spells an integer as ZON's reader writes a big
/// integer's digits, which is when the render writes a lone `$big` as the
/// integer itself: a minus sign at most, and first, then `0` or digits
/// that do not begin with `0`, but not `-0`.
fn zon_big_digits(s: &str) -> bool {
    let digits = s.strip_prefix('-').unwrap_or(s);
    !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'))
        && s != "-0"
}

/// The integer an object whose only member is `$big` spells, when its
/// value is a big integer's digits: ZON's render writes that object as
/// the integer, and its reader builds an integer no double holds exactly
/// as that object.
fn zon_big(d: &Datum) -> Option<Datum> {
    let members = d.as_object().filter(|m| m.len() == 1)?;
    match members.get("$big") {
        Some(Datum::String(s)) if zon_big_digits(s) => Some(Datum::Number {
            value: s.parse().ok()?,
            lexeme: Some(s.clone()),
        }),
        _ => None,
    }
}

/// What ZON's conventions make of a value it is given: an empty struct
/// reads back as an empty tuple, and a lone `$big` holding a big
/// integer's digits as that integer.
fn zon_reading(d: &Datum) -> Datum {
    match d {
        Datum::Object(m) if m.is_empty() => Datum::Array(Vec::new()),
        Datum::Object(_) if zon_big(d).is_some() => zon_big(d).unwrap(),
        Datum::Array(items) => Datum::Array(items.iter().map(zon_reading).collect()),
        Datum::Object(members) => Datum::Object(
            members
                .iter()
                .map(|(k, v)| (k.clone(), zon_reading(v)))
                .collect(),
        ),
        d => d.clone(),
    }
}

/// A field name as ZON's render wrote it, read back by the declared
/// reverse of its convention: `$empty` is the empty name; `$$` and a rest
/// is `$` and the rest; `$json:` and a text is the string the text spells
/// as a double-quoted JSON string; any other name is as it is.
fn zon_name(written: &str) -> Box<str> {
    if written == "$empty" {
        "".into()
    } else if let Some(rest) = written.strip_prefix("$$") {
        format!("${rest}").into()
    } else if let Some(json) = written.strip_prefix("$json:") {
        serde_json::from_str::<String>(json)
            .map(Into::into)
            .unwrap_or_else(|_| written.into())
    } else {
        written.into()
    }
}

/// What ZON's reader made of a document its render wrote, as the value it
/// was: a lone `$big` (the reader's big integer) is the integer, and every
/// field name reads back by the reverse of the convention that wrote it.
fn zon_back(d: &Datum) -> Datum {
    match d {
        Datum::Object(_) if zon_big(d).is_some() => zon_big(d).unwrap(),
        Datum::Array(items) => Datum::Array(items.iter().map(zon_back).collect()),
        Datum::Object(members) => Datum::Object(
            members
                .iter()
                .map(|(k, v)| (zon_name(k), zon_back(v)))
                .collect(),
        ),
        d => d.clone(),
    }
}

/// The default operators' source texts, which expr's render writes as the
/// operator (`(` a group): an operation is a list whose first element is
/// one, or an object whose `src` is one.
const EXPR_OPERATORS: [&str; 6] = ["+", "-", "*", "/", "%", "("];

/// Whether expr's render refuses a tree (its loss list): it holds a list
/// whose first element is an object with a `src` member a simplifier would
/// read as the list's operator, anything but the empty string, false,
/// null or zero, which is no default operator's source text, as the tokens
/// a C syntax tree puts first in a node's children are.
fn expr_refuses(d: &Datum) -> bool {
    match d {
        Datum::Array(items) => {
            let led = match items.first() {
                Some(Datum::Object(m)) => match m.get("src") {
                    None | Some(Datum::Null | Datum::Bool(false)) => false,
                    Some(Datum::String(src)) => {
                        !src.is_empty() && !EXPR_OPERATORS.contains(&&**src)
                    }
                    Some(Datum::Number { value, lexeme }) => {
                        *value != 0.0 || lexeme.as_deref().is_some_and(|l| l.starts_with('-'))
                    }
                    Some(_) => true,
                },
                _ => false,
            };
            led || items.iter().any(expr_refuses)
        }
        Datum::Object(members) => members.values().any(expr_refuses),
        _ => false,
    }
}

/// A tree as expr's reader reads one back (its simplified tree, the form
/// its shared fixtures hold): a list whose first element is an object whose
/// `src` is a default operator's source text has that text in the object's
/// place, as expr's loss list declares. Any other such `src` its render
/// refuses ([`expr_refuses`]), so no tree holding one is read back.
fn expr_simplify(d: &Datum) -> Datum {
    match d {
        Datum::Array(items) => Datum::Array(
            items
                .iter()
                .enumerate()
                .map(|(i, item)| match (i, item) {
                    (0, Datum::Object(m)) => match m.get("src") {
                        Some(Datum::String(src)) if EXPR_OPERATORS.contains(&&**src) => {
                            Datum::String(src.clone())
                        }
                        _ => expr_simplify(item),
                    },
                    _ => expr_simplify(item),
                })
                .collect(),
        ),
        Datum::Object(members) => Datum::Object(
            members
                .iter()
                .map(|(k, v)| (k.clone(), expr_simplify(v)))
                .collect(),
        ),
        d => d.clone(),
    }
}

/// What expr's conventions make of a tree it is given (its loss list): an
/// operator's description reads back as its source text; a negative
/// number, written with its sign, as the operator `-` applied to its
/// magnitude; and a number that is not finite as the string Infinity,
/// -Infinity or NaN.
fn expr_reading(d: &Datum) -> Datum {
    fn numbers(d: &Datum) -> Datum {
        match d {
            Datum::Number { value, .. } if value.is_nan() => Datum::String("NaN".into()),
            Datum::Number { value, .. } if value.is_infinite() => Datum::String(
                if *value > 0.0 {
                    "Infinity"
                } else {
                    "-Infinity"
                }
                .into(),
            ),
            Datum::Number { value, lexeme }
                if lexeme
                    .as_deref()
                    .map_or(value.is_sign_negative(), |l| l.starts_with('-')) =>
            {
                Datum::Array(vec![
                    Datum::String("-".into()),
                    Datum::Number {
                        value: value.abs(),
                        lexeme: None,
                    },
                ])
            }
            Datum::Array(items) => Datum::Array(items.iter().map(numbers).collect()),
            Datum::Object(members) => Datum::Object(
                members
                    .iter()
                    .map(|(k, v)| (k.clone(), numbers(v)))
                    .collect(),
            ),
            d => d.clone(),
        }
    }
    numbers(&expr_simplify(d))
}

/// The largest integer every runtime's reader of a version keeps as a
/// number, 2^53 - 1; Semantic Versioning's Rust reader keeps one past it
/// as its digits.
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// A version's number as Semantic Versioning's reader builds it from its
/// digits: a number up to 2^53 - 1, and its digits past it.
fn semver_number(digits: &str) -> Datum {
    match digits.parse::<f64>() {
        Ok(value) if value <= MAX_SAFE_INTEGER => Datum::Number {
            value,
            lexeme: None,
        },
        _ => Datum::String(digits.into()),
    }
}

/// What a version reads back as, by Semantic Versioning's loss list: its
/// major, minor and patch, each the number its digits make; its prerelease
/// and build as lists, empty where they are absent, null or empty, one
/// string split at its dots; a prerelease identifier of digits as the
/// number they make and a build identifier as its text; and no other
/// member.
fn semver_reading(d: &Datum) -> Datum {
    let m = d.as_object().expect("a version is an object");
    let text = |v: &Datum| match v {
        Datum::String(s) => s.to_string(),
        n => written_digits(n).expect("a version's numbers are written in digits"),
    };
    let identifiers = |key: &str, prerelease: bool| {
        let items: Vec<Datum> = match m.get(key) {
            Some(Datum::String(s)) if !s.is_empty() => {
                s.split('.').map(|id| Datum::String(id.into())).collect()
            }
            Some(Datum::Array(items)) => items.clone(),
            _ => Vec::new(),
        };
        Datum::Array(
            items
                .iter()
                .map(|id| {
                    let id = text(id);
                    if prerelease && id.bytes().all(|b| b.is_ascii_digit()) {
                        semver_number(&id)
                    } else {
                        Datum::String(id.into())
                    }
                })
                .collect(),
        )
    };
    let core = |key: &str| semver_number(&text(&m[key]));
    Datum::Object(
        [
            ("major", core("major")),
            ("minor", core("minor")),
            ("patch", core("patch")),
            ("prerelease", identifiers("prerelease", true)),
            ("build", identifiers("build", false)),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v))
        .collect(),
    )
}

/// The id, and an author's uri, the feed render supplies where a feed has
/// none.
const FEED_RENDER: &str = "tag:tabnas.dev,2026:feed-render";

/// The date the feed render supplies where a feed or an entry has none,
/// the one its embedding gives a plain tree.
const FEED_EPOCH: &str = "1970-01-01T00:00:00Z";

/// Whether `s` is `n` decimal digits.
fn digits_of(n: usize, s: &str) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_digit())
}

/// Whether `s` is two digits from `lo` to `hi`.
fn two_digits(lo: u32, hi: u32, s: &str) -> bool {
    digits_of(2, s) && s.parse::<u32>().is_ok_and(|n| lo <= n && n <= hi)
}

/// Whether `s` is an RFC 3339 date-time with the upper-case T and Z Atom
/// asks for, as the feed render reads one: a full date; a time, whose
/// seconds may have a fraction; and Z or an offset.
fn rfc3339(s: &str) -> bool {
    let time = |t: &str| {
        let p: Vec<&str> = t.split(':').collect();
        let second = |sec: &str| match sec.split('.').collect::<Vec<_>>()[..] {
            [whole] => two_digits(0, 60, whole),
            [whole, fraction] => {
                two_digits(0, 60, whole)
                    && !fraction.is_empty()
                    && fraction.bytes().all(|b| b.is_ascii_digit())
            }
            _ => false,
        };
        p.len() == 3 && two_digits(0, 23, p[0]) && two_digits(0, 59, p[1]) && second(p[2])
    };
    let offset = |o: &str| match o.split(':').collect::<Vec<_>>()[..] {
        [h, m] => two_digits(0, 23, h) && two_digits(0, 59, m),
        _ => false,
    };
    let [date, rest] = s.split('T').collect::<Vec<_>>()[..] else {
        return false;
    };
    let date_ok = match date.split('-').collect::<Vec<_>>()[..] {
        [y, m, d] => digits_of(4, y) && two_digits(1, 12, m) && two_digits(1, 31, d),
        _ => false,
    };
    date_ok
        && match rest.split('Z').collect::<Vec<_>>()[..] {
            [t, ""] => time(t),
            [_] => match rest.split('+').collect::<Vec<_>>()[..] {
                [t, o] => time(t) && offset(o),
                [_] => match rest.split('-').collect::<Vec<_>>()[..] {
                    [t, o] => time(t) && offset(o),
                    _ => false,
                },
                _ => false,
            },
            _ => false,
        }
}

/// An RSS date, RFC 822's date-time with a four-digit year allowed and its
/// names in any case, as the same instant in RFC 3339's form, as the feed
/// render writes one: a day of the week and a comma at most, then the day,
/// the month, the year (two digits before 50 in the 2000s, else in the
/// 1900s), the time (seconds 00 where it has none) and the zone (Z for UT,
/// GMT and Z, the US zones' offsets, or a sign and four digits). Its tabs
/// and line breaks are spaces.
fn rfc822(s: &str) -> Option<String> {
    let words = |t: &str| -> Vec<String> {
        t.replace(['\n', '\r', '\t'], " ")
            .split(' ')
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect()
    };
    let parts: Vec<&str> = s.split(',').collect();
    let w = match parts[..] {
        [_] => words(s),
        [day, rest] => match &words(day)[..] {
            [d] if ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]
                .contains(&d.to_ascii_lowercase().as_str()) =>
            {
                words(rest)
            }
            _ => return None,
        },
        _ => return None,
    };
    let [day, month, year, time, zone] = &w[..] else {
        return None;
    };
    let year = if digits_of(4, year) {
        year.clone()
    } else if digits_of(2, year) {
        format!("{}{year}", if year.as_str() < "50" { "20" } else { "19" })
    } else {
        return None;
    };
    let months = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let month = months
        .iter()
        .position(|m| *m == month.to_ascii_lowercase())
        .map(|i| format!("{:02}", i + 1))?;
    let day = if digits_of(1, day) {
        format!("0{day}")
    } else {
        day.clone()
    };
    if !two_digits(1, 31, &day) {
        return None;
    }
    let mut hms: Vec<&str> = time.split(':').collect();
    if hms.len() == 2 {
        hms.push("00");
    }
    if !(hms.len() == 3
        && two_digits(0, 23, hms[0])
        && two_digits(0, 59, hms[1])
        && two_digits(0, 60, hms[2]))
    {
        return None;
    }
    let numeric = |sign: &str, digits: &str| {
        (digits_of(4, digits) && two_digits(0, 23, &digits[..2]) && two_digits(0, 59, &digits[2..]))
            .then(|| format!("{sign}{}:{}", &digits[..2], &digits[2..]))
    };
    let offset = match zone.to_ascii_lowercase().as_str() {
        "ut" | "gmt" | "z" => Some("Z".to_string()),
        "est" => Some("-05:00".to_string()),
        "edt" => Some("-04:00".to_string()),
        "cst" => Some("-06:00".to_string()),
        "cdt" => Some("-05:00".to_string()),
        "mst" => Some("-07:00".to_string()),
        "mdt" => Some("-06:00".to_string()),
        "pst" => Some("-08:00".to_string()),
        "pdt" => Some("-07:00".to_string()),
        // A sign and four digits; the render splits the zone at its signs,
        // so one with a second sign is no zone.
        _ => match (zone.strip_prefix('+'), zone.strip_prefix('-')) {
            (Some(digits), _) if !digits.contains('+') => numeric("+", digits),
            (_, Some(digits)) if !zone.contains('+') && !digits.contains('-') => {
                numeric("-", digits)
            }
            _ => None,
        },
    }?;
    Some(format!("{year}-{month}-{day}T{}{offset}", hms.join(":")))
}

/// A date as the feed render writes it: its RFC 3339 form (`Ok`), or the
/// text of one in neither form (`Err`), which it writes as the epoch with
/// a category that keeps the text. A date with no text is a missing one.
fn feed_date(v: &Datum) -> Result<String, String> {
    match v {
        Datum::String(s) if s.is_empty() => Ok(FEED_EPOCH.to_string()),
        Datum::String(s) => {
            let upper = s.replace('t', "T").replace('z', "Z");
            if rfc3339(&upper) {
                Ok(upper)
            } else {
                rfc822(s).ok_or_else(|| s.to_string())
            }
        }
        v => Err(v.to_string()),
    }
}

/// A feed's value as the reader builds it back: no member whose value is
/// null, and a character XML 1.0 cannot carry as U+FFFD.
fn feed_clean(d: &Datum) -> Datum {
    match d {
        Datum::String(s) => Datum::String(
            s.chars()
                .map(|c| match c {
                    '\t' | '\n' | '\r' => c,
                    c if (c as u32) < 0x20 || c == '\u{fffe}' || c == '\u{ffff}' => '\u{fffd}',
                    c => c,
                })
                .collect::<String>()
                .into(),
        ),
        Datum::Array(items) => Datum::Array(items.iter().map(feed_clean).collect()),
        Datum::Object(members) => Datum::Object(
            members
                .iter()
                .filter(|(_, v)| !matches!(v, Datum::Null))
                .map(|(k, v)| (k.clone(), feed_clean(v)))
                .collect(),
        ),
        d => d.clone(),
    }
}

/// A text construct or a content as the feed render writes it: one of type
/// xhtml as html, its value trimmed.
fn feed_text(d: &Datum) -> Datum {
    let cleaned = feed_clean(d);
    match &cleaned {
        Datum::Object(m) if m.get("type").and_then(Datum::as_str) == Some("xhtml") => {
            Datum::Object(
                m.iter()
                    .map(|(k, v)| match (&**k, v) {
                        ("type", _) => (k.clone(), Datum::String("html".into())),
                        ("value", Datum::String(s)) => (k.clone(), Datum::String(s.trim().into())),
                        _ => (k.clone(), v.clone()),
                    })
                    .collect(),
            )
        }
        _ => cleaned,
    }
}

/// Whether a feed or an entry has an author: a list of them that is not
/// empty.
fn feed_has_author(m: &Datum) -> bool {
    matches!(m.as_object().and_then(|m| m.get("authors")), Some(Datum::Array(a)) if !a.is_empty())
}

/// A feed's or an entry's members as the feed render writes them and the
/// reader builds them back (`feed_reading`): each date in RFC 3339's form,
/// one the render cannot read as the epoch with a category keeping its
/// text, which comes before the object's own categories where the date
/// comes before them in the tree's order; text constructs as `feed_text`;
/// an id, a title and an updated where the object has none, an entry's id
/// with a slash and `position`; an entry's source not read back.
fn feed_object(m: &Datum, position: Option<usize>) -> Vec<(Box<str>, Datum)> {
    let members = m.as_object().expect("a feed and an entry are objects");
    let mut out: Vec<(Box<str>, Datum)> = Vec::new();
    let (mut before, mut after) = (Vec::new(), Vec::new());
    let mut categories_met = false;
    for (k, v) in members {
        if matches!(v, Datum::Null) {
            continue;
        }
        match &**k {
            "categories" => categories_met = true,
            "source" | "format" | "version" | "entries" => continue,
            _ => {}
        }
        let value = match &**k {
            "updated" | "published" => match feed_date(v) {
                Ok(date) => Datum::String(date.into()),
                Err(text) => {
                    let category = Datum::Object(
                        [
                            ("term", Datum::String(k.clone())),
                            (
                                "scheme",
                                Datum::String(format!("{FEED_RENDER}/date").into()),
                            ),
                            ("label", feed_clean(&Datum::String(text.into()))),
                        ]
                        .into_iter()
                        .map(|(k, v)| (k.into(), v))
                        .collect(),
                    );
                    if categories_met {
                        after.push(category);
                    } else {
                        before.push(category);
                    }
                    Datum::String(FEED_EPOCH.into())
                }
            },
            "title" | "subtitle" | "rights" | "summary" | "content" => feed_text(v),
            _ => feed_clean(v),
        };
        out.push((k.clone(), value));
    }
    if !before.is_empty() || !after.is_empty() {
        let own = match out.iter().position(|(k, _)| &**k == "categories") {
            Some(i) => match out.remove(i).1 {
                Datum::Array(items) => items,
                _ => Vec::new(),
            },
            None => Vec::new(),
        };
        let all = before.into_iter().chain(own).chain(after).collect();
        out.push(("categories".into(), Datum::Array(all)));
    }
    let has = |out: &[(Box<str>, Datum)], key: &str| out.iter().any(|(k, _)| &**k == key);
    if !has(&out, "id") {
        let id = match position {
            Some(i) => format!("{FEED_RENDER}/{i}"),
            None => FEED_RENDER.to_string(),
        };
        out.push(("id".into(), Datum::String(id.into())));
    }
    if !has(&out, "title") {
        let title = Datum::Object(
            [("type", "text"), ("value", "")]
                .into_iter()
                .map(|(k, v)| (k.into(), Datum::String(v.into())))
                .collect(),
        );
        out.push(("title".into(), title));
    }
    if !has(&out, "updated") {
        out.push(("updated".into(), Datum::String(FEED_EPOCH.into())));
    }
    out
}

/// What a feed reads back as, by its loss list: an Atom 1.0 feed of its
/// members and its entries as `feed_object` writes them, and an author,
/// named by the feed's title where that is text and not empty and
/// `unknown` otherwise, where the feed has none, though every entry may
/// have one: the render writes a feed's elements before its entries, each
/// entry as it comes, so it cannot see them first.
fn feed_reading(d: &Datum) -> Datum {
    let members = d.as_object().expect("a feed is an object");
    let entries: Vec<Datum> = match members.get("entries") {
        Some(Datum::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(i, e)| Datum::Object(feed_object(e, Some(i)).into_iter().collect()))
            .collect(),
        _ => Vec::new(),
    };
    let mut out = vec![
        ("format".into(), Datum::String("atom".into())),
        ("version".into(), Datum::String("1.0".into())),
    ];
    if members.contains_key("entries") {
        out.push(("entries".into(), Datum::Array(entries)));
    }
    out.extend(feed_object(d, None));
    if !feed_has_author(d) {
        let name = match members.get("title") {
            Some(Datum::Object(t))
                if t.get("type").and_then(Datum::as_str) == Some("text")
                    && t.get("value")
                        .and_then(Datum::as_str)
                        .is_some_and(|v| !v.is_empty()) =>
            {
                t["value"].clone()
            }
            _ => Datum::String("unknown".into()),
        };
        let author = Datum::Object(
            [("name", name), ("uri", Datum::String(FEED_RENDER.into()))]
                .into_iter()
                .map(|(k, v)| (k.into(), v))
                .collect(),
        );
        out.retain(|(k, _)| &**k != "authors");
        out.push(("authors".into(), Datum::Array(vec![author])));
    }
    Datum::Object(out.into_iter().collect())
}

/// Whether a grammar spec written in a grammar notation reads back as the
/// notations' conventions say. A render writes the spec anew, as far as
/// its notation can say it, and its contract is the round trip (each
/// render's header): the text compiles back to the spec it was written
/// from, but where its loss list says it compiles back to another (a spec
/// whose alternatives the compiler reordered, or whose left recursion ran
/// through another rule), and a spec compiled from another notation,
/// which compiles back under the target's own settings (its group tag, its
/// lexing, its spelling of the other notation's terminals and core rules)
/// and recognises what it recognised. So a spec of the target's own
/// notation reads back as it was, or as one the render writes again as
/// the same text; and one of another notation reads back as a spec the
/// render writes again as text that reads back as that spec.
fn check_grammar(
    from: &Format,
    target: &Format,
    source: &Datum,
    written: &str,
) -> Result<(), String> {
    let limits = Limits::default();
    let back = target
        .read(written, &limits)
        .map_err(|f| format!("the written grammar does not read back: {f}"))?;
    if from.id() == target.id() && same(source, &back) {
        return Ok(());
    }
    let again = translate_text(target, target, written, None).map_err(|f| {
        format!(
            "the spec it reads back as is not written again: {f}; it was written as {written:?}"
        )
    })?;
    if from.id() == target.id() {
        return if again == written {
            Ok(())
        } else {
            Err(format!(
                "reads back as another spec, which is written again as {again:?}, where \
                 {written:?} was written"
            ))
        };
    }
    let back_again = target
        .read(&again, &limits)
        .map_err(|f| format!("the grammar written again does not read back: {f}"))?;
    if same(&back, &back_again) {
        Ok(())
    } else {
        Err(format!(
            "reads back as a spec that is written again as {again:?}, which reads back as \
             another, where {written:?} was written"
        ))
    }
}

/// Whether a grammar spec sets the lexing GBNF's compiler gives a spec:
/// exact, no white space skipped and no matcher of the engine's own
/// (`space.lex` off).
fn exact_lexing(spec: &Datum) -> bool {
    spec.as_object()
        .and_then(|spec| spec.get("options"))
        .and_then(Datum::as_object)
        .and_then(|options| options.get("space"))
        .and_then(Datum::as_object)
        .and_then(|space| space.get("lex"))
        == Some(&Datum::Bool(false))
}

/// The engine with a grammar spec installed, as a fresh instance each:
/// installing applies the spec's lexer options to the instance.
fn grammar_engine(spec: &Datum) -> Result<tabnas::Tabnas, String> {
    let mut engine = tabnas::Tabnas::new();
    engine
        .grammar_json(&spec.to_string())
        .map_err(|e| format!("the spec does not install: {e}"))?;
    Ok(engine)
}

/// What a grammar spec, `source`, written in `target` as `written`
/// recognises against what the spec that text compiles to recognises, over
/// the document's samples: each sample is parsed with both, and both accept
/// it or both refuse it. A render writes a spec as far as its notation can
/// say it and recognises what it recognised (each loss list's sentence on
/// the tree builders), but for the lexing: a spec of another notation
/// compiles back under the target's own settings (its loss list), so across
/// GBNF's exact lexing and the others' default one, which skips white
/// space, a sample holding white space may be recognised otherwise, as
/// declared; the caller holds those to the samples `test/notation-samples.json`
/// registers for the pair. Ok is the number of samples compared and the
/// samples recognised otherwise across the lexing.
fn recognition(
    target: &Format,
    source: &Datum,
    written: &str,
    samples: &[String],
) -> Result<(usize, Vec<String>), String> {
    if samples.is_empty() {
        return Ok((0, Vec::new()));
    }
    let back = target
        .read(written, &Limits::default())
        .map_err(|f| format!("the written grammar does not read back: {f}"))?;
    let across = exact_lexing(source) != exact_lexing(&back);
    let (read, written_engine) = (grammar_engine(source)?, grammar_engine(&back)?);
    let mut otherwise = Vec::new();
    for sample in samples {
        let (was, is) = (
            read.parse(sample).is_ok(),
            written_engine.parse(sample).is_ok(),
        );
        if was == is {
            continue;
        }
        if across && sample.contains([' ', '\t', '\n', '\r']) {
            otherwise.push(sample.clone());
            continue;
        }
        return Err(format!(
            "recognises {sample:?} otherwise: the grammar read {} it, the one written {} it, \
             where {written:?} was written",
            if was { "accepts" } else { "refuses" },
            if is { "accepts" } else { "refuses" },
        ));
    }
    Ok((samples.len(), otherwise))
}

/// Whether the document read back from `written` in `target` is what the
/// target's conventions make of `source`, read as `from`.
fn check(from: &Format, target: &Format, source: &Datum, written: &str) -> Result<(), String> {
    if grammar_notation(target) {
        return check_grammar(from, target, source, written);
    }
    let limits = Limits::default();
    let id = target.id();
    // A source whose events carry the target's own schema (XML's element
    // tree into XML) is written as it is, and reads back as it is.
    let embedded = target.part.schema.is_some() && from.part.schema != target.part.schema;
    let back = if id == "markdown" {
        // A Markdown table reads back as records through the format's
        // lift, as a host reads it for a records target.
        let lift = target.part.lift.as_ref().expect("markdown has a lift");
        let program = format!(
            "{}\ndef export [input] (records ({} input))\n",
            lift.text, lift.entry
        );
        let json = translate_text(
            target,
            format("yaml"),
            written,
            Some(("markdown-records.alc", &program)),
        )
        .map_err(|f| format!("the written table does not read back: {f}"))?;
        format("yaml")
            .read(&json, &limits)
            .map_err(|f| format!("its records do not read back: {f}"))?
    } else if (id == "xml" || id == "feed") && embedded {
        // An embedding reads back through its reverse, which its file
        // holds beside it (`xml-unembed`, `feed-unembed`): the format's
        // tree, as JSON, unembedded, and written where a non-finite number
        // has a spelling.
        let tree = target
            .read(written, &limits)
            .map_err(|f| format!("the written document does not read back: {f}"))?;
        let embed = target
            .part
            .embed
            .as_ref()
            .expect("an embedding target has an embed");
        let reverse = embed
            .entry
            .strip_suffix("-embed")
            .map(|name| format!("{name}-unembed"))
            .expect("an embed's entry is named NAME-embed");
        let program = format!("{}\ndef export [input] ({reverse} input)\n", embed.text);
        let yaml = translate_text(
            format("json"),
            format("yaml"),
            &tree.to_string(),
            Some((&format!("{reverse}.alc"), &program)),
        )
        .map_err(|f| format!("the tree does not unembed: {f}"))?;
        format("yaml")
            .read(&yaml, &limits)
            .map_err(|f| format!("the unembedded tree does not read back: {f}"))?
    } else {
        target
            .read(written, &limits)
            .map_err(|f| format!("the written document does not read back: {f}"))?
    };
    let key = Options::default().key;
    let expected = match id {
        "csv" => return check_records(source, &back, &|t| t.to_string()),
        "markdown" => return check_records(source, &back, &markdown_cell),
        "ini" => {
            let expected = wrap_object(source, &Options::default().key);
            return if ini_same(&expected, &back) {
                Ok(())
            } else {
                Err(format!("read back as {back}, where {expected} was written"))
            };
        }
        "json" | "jsonc" | "jsonic" => map_non_finite(source, &|_| Datum::Null),
        "jsonl" => map_non_finite(&wrap_array(source), &|_| Datum::Null),
        "toml" => without_nulls(&wrap_object(source, &key)),
        "zon" => zon_reading(source),
        "expr" => expr_reading(source),
        "semver" => semver_reading(source),
        // A feed's own tree, written as Atom 1.0, with no member whose
        // value is null (an embedded one reads back through its reverse,
        // above, as it was).
        "feed" if !embedded => feed_reading(source),
        _ => source.clone(),
    };
    // A null member a feed reads back with is one the render left out.
    let back = if id == "feed" && !embedded {
        feed_clean(&back)
    } else {
        back
    };
    let back = if id == "zon" { zon_back(&back) } else { back };
    if same(&expected, &back) {
        Ok(())
    } else {
        Err(format!("read back as {back}, where {expected} was written"))
    }
}

/// How deep a value nests: a scalar is 0, a container one more than its
/// deepest member.
fn depth(d: &Datum) -> usize {
    match d {
        Datum::Array(items) => 1 + items.iter().map(depth).max().unwrap_or(0),
        Datum::Object(members) => 1 + members.values().map(depth).max().unwrap_or(0),
        _ => 0,
    }
}

/// The nesting every format reads: the readers guard nesting at different
/// depths (tabnas-json past 128 levels, YAML's and ZON's near it, the
/// transducer at 256 events deep, which XML's embedding reaches at about
/// 127 levels, two elements a level), and a root adapter or an embedding
/// adds a level or two. A document nested deeper than this is at one
/// format's guard and past another's, a limit and not a shape, so the
/// matrix leaves it out, and pins how few such documents there are.
const DEPTH_BOUND: usize = 100;

/// How many values a tree holds: a scalar is one, a container one more
/// than its members hold.
fn size(d: &Datum) -> usize {
    match d {
        Datum::Array(items) => 1 + items.iter().map(size).sum::<usize>(),
        Datum::Object(members) => 1 + members.values().map(size).sum::<usize>(),
        _ => 1,
    }
}

/// The size of a tree every format writes in moments, in the release run.
/// The grammar spec RFC 3986's URI grammar compiles to holds 513,409 values
/// (its probe tables), which take seconds into JSON and minutes into an XML
/// embedding even in release: a workload and not a shape, so the matrix
/// leaves a document past its bound out, and pins how few such documents
/// there are. The runs every runtime makes by default hold a tree to a
/// tenth of this (`DEFAULT_SIZE_BOUND`).
const SIZE_BOUND: usize = 100_000;

/// The size of a tree every format writes in moments in the TypeScript
/// command too, whose interpreter takes minutes over the 14,000 values of
/// the grammar specs the larger GBNF examples compile to (C's, JSON's):
/// the bound of the runs every runtime makes by default, and so of the
/// TypeScript and Go matrices, which read the same corpora.
const DEFAULT_SIZE_BOUND: usize = 10_000;

/// What a run of the matrix is held to: at least `floor` documents, at
/// most `too_deep` of them deeper than every format reads and `too_large`
/// larger than `size` values, at least `refusals` pairs refused as their
/// target declares, at least `grammars_written` grammars written in a
/// grammar notation, at least `samples_compared` samples of theirs
/// (`samples`, by document) compared between the grammar read and the one
/// written, each recognised by both alike but those `otherwise` registers
/// for the pair (every pair it registers one the run compares, when
/// `every_otherwise_met`), and the pairs of its corpus that fail for a
/// defect of their target's package (`defective_pairs`, `DEFECTIVE_PAIRS`).
struct Bounds {
    floor: usize,
    too_deep: usize,
    size: usize,
    too_large: usize,
    /// The documents a source's part refuses part way, at most: each is
    /// read with `--whole`, and its pairs are held to what that writes.
    whole: usize,
    refusals: usize,
    grammars_written: usize,
    defective_pairs: &'static [(&'static str, &'static str)],
    samples: HashMap<String, Vec<String>>,
    samples_compared: usize,
    otherwise: HashMap<String, Vec<String>>,
    every_otherwise_met: bool,
}

/// The pairs of the release run that fail for a defect of their source's
/// or target's package, outside this repository: by the name the matrix
/// gives a pair, each with its defect (Go's matrix keeps the like,
/// `divergent`). The matrix holds each to failing, so an entry cannot
/// outlive the defect it records: one that passes, or that names no pair
/// of the corpus, fails the test until it is deleted. Only the release run
/// reads the documents. None is open. tabnas-yaml 0.5.28 fixed the last
/// source's defect, with transduce's hold on its pruning: the fourteen
/// pairs of `yaml/anchors-aliases.tsv:37`, whose alias to an anchored
/// sequence used as a key read through YAML's part as the text of the
/// sequence emptied behind the stream (tabnas/yaml#138). tabnas-ebnf 0.1.18
/// fixed the last target's, `abnf/alignment-abnf-ast.tsv:28 (abnf) ->
/// ebnf`, whose bounded repetition (`*200"a"`) its render wrote as 200
/// nested optional groups, past what its own reader reads; the render now
/// refuses such a repetition, as its loss list declares.
const DEFECTIVE_PAIRS: [(&str, &str); 0] = [];

/// The cross product of `docs` and every format: each document read with
/// its format's grammar, written in every format, and read back under the
/// target's conventions, or refused as the target declares, held to
/// `bounds`; a document of a format whose reader is a registered defect is
/// left out as a source, and counted.
fn matrix(docs: Vec<(String, &'static str, String)>, bounds: Bounds) {
    let limits = Limits::default();
    let targets = translate::formats();
    assert_eq!(targets.len(), 22, "the formats: {}", translate::names());
    let total = docs.len() * targets.len();
    let mut failures: Vec<String> = Vec::new();
    let mut refused_sources = Vec::new();
    let mut too_deep = Vec::new();
    let mut too_large = Vec::new();
    let mut defective = Vec::new();
    let mut refusals: Vec<String> = Vec::new();
    let mut grammars_written = 0;
    let mut samples_compared = 0;
    let mut declared_otherwise = Vec::new();
    let mut otherwise_met = HashSet::new();
    let mut diverged = Vec::new();
    let mut repaired = Vec::new();
    let mut met = HashSet::new();
    let mut pairs = 0;
    let mut read_whole: Vec<(String, Code)> = Vec::new();
    let started = Instant::now();
    let mut reported = Instant::now();
    for (n, (name, from, text)) in docs.iter().enumerate() {
        let from = format(from);
        let source = if registered_defect(from.id()) {
            defective.push(name.clone());
            None
        } else {
            match from.read(text, &limits) {
                Err(f) => {
                    refused_sources.push(format!("{name}: {f}"));
                    None
                }
                Ok(d) if depth(&d) > DEPTH_BOUND => {
                    too_deep.push(format!("{name}: {} levels", depth(&d)));
                    None
                }
                Ok(d) if size(&d) > bounds.size => {
                    too_large.push(format!("{name}: {} values", size(&d)));
                    None
                }
                Ok(d) => Some(d),
            }
        };
        for to in targets.iter().filter(|_| source.is_some()) {
            let source = source.as_ref().expect("a document read");
            pairs += 1;
            let pair = format!("{name} ({}) -> {}", from.id(), to.id());
            let mut written = translate_text(from, to, text, None);
            // A document the source's part cannot stream fails, naming
            // --whole; read whole, the pair is held to what that writes, and
            // the document is counted.
            if let Err(f) = &written {
                if unstreamed(f) {
                    let doc = format!("{name} ({})", from.id());
                    if !read_whole.iter().any(|(d, _)| *d == doc) {
                        read_whole.push((doc, f.code));
                    }
                    written = translate_whole(from, to, text);
                }
            }
            let outcome = match (expect(from, to, source), written) {
                (Expect::Refused(code, reason), Err(f))
                    if f.code == code && f.message.starts_with(&reason) =>
                {
                    refusals.push(format!("{pair}: {reason}"));
                    Ok(())
                }
                (Expect::Refused(code, reason), Err(f)) => Err(format!(
                    "is refused otherwise than declared ({code:?}, {reason}...): {f}"
                )),
                (Expect::Refused(code, reason), Ok(written)) => Err(format!(
                    "is written, where it is declared refused ({code:?}, {reason}...): {written:?}"
                )),
                (Expect::WrittenUnlessRefused(code, reason), Err(f))
                    if f.code == code && f.message.starts_with(&reason) =>
                {
                    refusals.push(format!("{pair}: {}", f.message));
                    Ok(())
                }
                (Expect::WrittenUnlessRefused(..), Err(f)) => Err(format!("does not write: {f}")),
                (Expect::WrittenUnlessRefused(..), Ok(written)) => {
                    grammars_written += 1;
                    let samples = bounds.samples.get(name).map_or(&[][..], Vec::as_slice);
                    check(from, to, source, &written).and_then(|()| {
                        let (compared, otherwise) = recognition(to, source, &written, samples)?;
                        samples_compared += compared;
                        let registered = bounds.otherwise.get(&pair).map_or(&[][..], Vec::as_slice);
                        if otherwise != registered {
                            return Err(format!(
                                "recognises {otherwise:?} otherwise across the lexing, where \
                                 test/notation-samples.json registers {registered:?} for the pair"
                            ));
                        }
                        if !otherwise.is_empty() {
                            otherwise_met.insert(pair.clone());
                            declared_otherwise
                                .push(format!("{pair}: {} of {compared} samples", otherwise.len()));
                        }
                        Ok(())
                    })
                }
                (Expect::Written, Err(f)) => Err(format!("does not write: {f}")),
                (Expect::Written, Ok(written)) => {
                    // A records source read through its lift writes its
                    // table, not its tree: compare with the table.
                    let lifted = from.part.reads.first()
                        == Some(&tabnas_alchemy::translate::Shape::Records)
                        && to.part.writes == tabnas_alchemy::translate::Shape::Records;
                    if lifted {
                        to.read(&written, &limits)
                            .map(|_| ())
                            .map_err(|f| format!("the written document does not read back: {f}"))
                    } else {
                        check(from, to, source, &written)
                    }
                }
            };
            let known = bounds
                .defective_pairs
                .iter()
                .find(|(name, _)| *name == pair);
            if let Some((name, _)) = known {
                met.insert(*name);
            }
            match (outcome, known) {
                (Err(why), Some(_)) => diverged.push(format!("{pair}: {why}")),
                (Err(why), None) => failures.push(format!("{pair}: {why}")),
                (Ok(()), Some(_)) => repaired.push(pair),
                (Ok(()), None) => {}
            }
        }
        if (n + 1) % 25 == 0 || n + 1 == docs.len() || reported.elapsed().as_secs() >= 20 {
            reported = Instant::now();
            eprintln!(
                "matrix: {} of {} documents ({}%), {} failures, {}s",
                n + 1,
                docs.len(),
                (n + 1) * 100 / docs.len(),
                failures.len(),
                started.elapsed().as_secs()
            );
        }
    }
    for line in &refused_sources {
        eprintln!("refused source: {line}");
    }
    for line in &too_deep {
        eprintln!("deeper than every format reads: {line}");
    }
    for line in &too_large {
        eprintln!("larger than {} values: {line}", bounds.size);
    }
    for line in &declared_otherwise {
        eprintln!(
            "recognised otherwise across the lexing, as the loss lists declare and \
             test/notation-samples.json registers: {line} holding white space"
        );
    }
    for (doc, code) in &read_whole {
        eprintln!("read with --whole, which its part cannot stream: {doc}: {code:?}");
    }
    for line in &diverged {
        eprintln!("defective, as registered: {line}");
    }
    for line in &failures {
        eprintln!("FAIL {line}");
    }
    let schema_only = refusals
        .iter()
        .filter(|r| r.contains("schema_only:"))
        .count();
    let unwritable = refusals
        .iter()
        .filter(|r| r.contains(": the grammar spec cannot be written as "))
        .count();
    let expr_refused = refusals
        .iter()
        .filter(|r| r.contains(": the object at "))
        .count();
    eprintln!(
        "matrix: {pairs} pairs of {} documents; {} refused by their own reader, {} too deep, \
         {} too large, {} left out for a registered reader defect, {} defective as registered, {} \
         read with --whole, which their parts cannot stream; {} pairs refused as their target \
         declares \
         ({schema_only} by a schema-only target, {unwritable} by a grammar notation's render, \
         {expr_refused} by expr's render, {} by Semantic Versioning's embedding); \
         {grammars_written} grammars written in a grammar notation, {samples_compared} of their \
         samples compared, {} pairs recognising some otherwise across the lexing, as registered",
        docs.len(),
        refused_sources.len(),
        too_deep.len(),
        too_large.len(),
        defective.len(),
        diverged.len(),
        read_whole.len(),
        refusals.len(),
        refusals.len() - schema_only - unwritable - expr_refused,
        declared_otherwise.len(),
    );
    let left_out = refused_sources.len() + too_deep.len() + too_large.len() + defective.len();
    assert!(
        pairs + left_out * targets.len() == total && docs.len() >= bounds.floor,
        "the corpora shrank: {} documents",
        docs.len()
    );
    assert!(
        too_deep.len() <= bounds.too_deep,
        "{} documents are deeper than every format reads (above)",
        too_deep.len()
    );
    assert!(
        read_whole.len() <= bounds.whole,
        "{} documents are read with --whole, more than the {} the corpora give (above)",
        read_whole.len(),
        bounds.whole
    );
    assert!(
        too_large.len() <= bounds.too_large,
        "{} documents hold more than {} values (above)",
        too_large.len(),
        bounds.size
    );
    assert!(
        refusals.len() >= bounds.refusals,
        "{} pairs are refused as their target declares, fewer than the {} the corpora give",
        refusals.len(),
        bounds.refusals
    );
    for (name, defect) in bounds.defective_pairs {
        assert!(
            met.contains(name),
            "the registered defective pair {name:?} is no pair of the corpus: delete its entry ({defect})"
        );
    }
    assert!(
        repaired.is_empty(),
        "the registered defective pairs {repaired:?} translate as the conventions say: delete their \
         entries"
    );
    assert!(
        samples_compared >= bounds.samples_compared,
        "{samples_compared} samples are compared, fewer than the {} the corpora give",
        bounds.samples_compared
    );
    if bounds.every_otherwise_met {
        let mut unmet: Vec<&String> = bounds
            .otherwise
            .keys()
            .filter(|pair| !otherwise_met.contains(*pair))
            .collect();
        unmet.sort();
        assert!(
            unmet.is_empty(),
            "test/notation-samples.json registers {unmet:?} as recognising samples otherwise, \
             pairs this run does not compare: delete their entries"
        );
    }
    assert!(
        grammars_written >= bounds.grammars_written,
        "{grammars_written} grammars are written in a grammar notation, fewer than the {} the \
         corpora give",
        bounds.grammars_written
    );
    assert!(
        failures.is_empty(),
        "{} of {pairs} pairs failed (above)",
        failures.len()
    );
}

/// Run `work` on a thread of the stack the command runs every command on
/// (`tabnas_alchemy::STACK_BYTES`), which the evaluator's bounds are
/// promised: a grammar notation's render is an alchemy program that
/// recurses deeper than a test thread's own stack holds.
fn on_stack<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(tabnas_alchemy::STACK_BYTES)
            .spawn_scoped(scope, work)
            .unwrap()
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

/// transduce's fixtures, one document per format at least, and the
/// documents of JSONTestSuite every JSON parser must accept.
#[test]
fn every_document_translates_into_every_format() {
    let bounds = Bounds {
        floor: 132,
        too_deep: 0,
        size: DEFAULT_SIZE_BOUND,
        too_large: 0,
        whole: 11,
        refusals: 1000,
        grammars_written: 0,
        defective_pairs: &[],
        samples: HashMap::new(),
        samples_compared: 0,
        otherwise: HashMap::new(),
        every_otherwise_met: false,
    };
    on_stack(|| matrix(corpus(), bounds));
}

/// Every format's own fixture corpus, and the grammar notations' example
/// grammars, into every format: thousands of documents, run in release by
/// `ci/rust/run.sh` (`--ignored`), where it takes minutes rather than the
/// hours a debug build would.
#[test]
#[ignore = "the cross product of every format's fixtures: ci/rust/run.sh runs it in release"]
fn every_fixture_of_every_format_translates_into_every_format() {
    let mut docs = Vec::new();
    let Notation {
        mut samples,
        otherwise,
    } = notation_samples();
    spec_corpus(&mut docs, &mut samples);
    let bounds = Bounds {
        floor: 4000,
        too_deep: 2,
        size: SIZE_BOUND,
        too_large: 1,
        whole: 85,
        refusals: 29898,
        grammars_written: 132,
        defective_pairs: &DEFECTIVE_PAIRS,
        samples,
        samples_compared: 456,
        otherwise,
        every_otherwise_met: true,
    };
    on_stack(|| matrix(docs, bounds));
}

/// Every document of the corpora translates from its file as from its
/// text, into JSON and into CSV, written or refused alike. A file is read
/// through the transducer part's route where its format's package ships a
/// part: through a reader, a window at a time where the grammar declares
/// `lex.window` (the sibling checkout's package says which), and whole
/// first otherwise. A document the part refuses fails from its file as from its
/// text, since nothing reads a document again.
#[test]
fn every_document_translates_from_its_file_as_from_its_text() {
    let dir = std::env::temp_dir().join(format!("dmog-{}-files", std::process::id()));
    std::fs::create_dir_all(&dir).expect("the directory is made");
    let docs = corpus();
    let compared = on_stack(|| {
        let mut compared = 0;
        for (index, (name, id, text)) in docs.iter().enumerate() {
            let from = format(id);
            let file: &Path = &dir.join(format!("{index}.{id}"));
            std::fs::write(file, text).expect("the file is written");
            for to in ["json", "csv"] {
                let to = format(to);
                let whole = translate_text(from, to, text, None).map_err(|f| format!("{f:?}"));
                let read = translate_document(from, to, Document::File(file), None, false)
                    .map_err(|f| format!("{f:?}"));
                assert_eq!(read, whole, "{name} into {}", to.id());
                compared += 1;
            }
        }
        compared
    });
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(compared, 2 * docs.len());
    assert!(docs.len() >= 132, "{}", docs.len());
}

/// A request from `from` to `to`, with no path and no program.
fn request<'f>(from: &'f Format, to: &'f Format) -> Request<'f> {
    Request {
        from,
        to,
        path: None,
        options: Options::default(),
        program: None,
        whole: false,
        limits: Limits::default(),
    }
}

/// ZON's reader builds an integer no double holds exactly as the object
/// `{"$big": "<digits>"}`, the digits after a minus sign when it is
/// negative, and ZON's render writes that object back as the integer: the
/// form every port reads it in.
#[test]
fn a_zon_integer_no_double_holds_is_the_big_object() {
    let (zon, json) = (format("zon"), format("json"));
    let text = ".{ 1, 12345678901234567890, -12345678901234567890, 0xc1ce108124179e16 }\n";
    let want = r#"[1,{"$big":"12345678901234567890"},{"$big":"-12345678901234567890"},{"$big":"13965117641364839958"}]"#;
    assert_eq!(
        translate_text(zon, json, text, None).unwrap().trim_end(),
        want
    );
    let back = translate_text(zon, zon, text, None).unwrap();
    assert_eq!(
        translate_text(zon, json, &back, None).unwrap().trim_end(),
        want
    );
}

/// The metrics a run is given count what it wrote, whether it streamed
/// through the transducer part or read the document whole (`--whole`),
/// and a run the part gives up writes nothing twice: it fails, its metrics
/// what it had written.
#[test]
fn a_runs_metrics_are_its_own() {
    let json = format("json");
    for (text, whole) in [(r#"{"a": [1, 2]}"#, false), (r#"{"a": 1, "a": 2}"#, true)] {
        let out = Buffer::default();
        let metrics = Metrics::new();
        let request = Request {
            whole,
            ..request(json, json)
        };
        translate::run(&request, text, Box::new(out.clone()), metrics.clone()).unwrap();
        let written = out.0.lock().unwrap().len() as u64;
        assert!(written > 0, "{text}");
        assert_eq!(Metrics::get(&metrics.output_bytes), written, "{text}");
        assert!(Metrics::get(&metrics.events) > 0, "{text}");
    }
    let out = Buffer::default();
    let metrics = Metrics::new();
    let fail = translate::run(
        &request(json, json),
        r#"{"a": 1, "a": 2}"#,
        Box::new(out.clone()),
        metrics.clone(),
    )
    .unwrap_err();
    assert_eq!(fail.code, Code::DuplicateMember, "{fail:?}");
    assert_eq!(
        Metrics::get(&metrics.output_bytes),
        out.0.lock().unwrap().len() as u64
    );
}

/// A writer that fails once it has taken some of the output: the failure
/// says the output had left, and says it had not when it took none.
#[test]
fn an_output_failure_after_bytes_left_is_committed() {
    struct Takes(usize);
    impl Write for Takes {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.0 == 0 {
                return Err(std::io::Error::other("the disk is full"));
            }
            let n = self.0.min(bytes.len());
            self.0 -= n;
            Ok(n)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let json = format("json");
    for (takes, committed) in [(0, false), (3, true)] {
        let fail = translate::run(
            &request(json, json),
            "[1, 2, 3]",
            Box::new(Takes(takes)),
            Metrics::new(),
        )
        .unwrap_err();
        assert_eq!(fail.code, Code::OutputFailed, "{takes}");
        assert_eq!(fail.committed_output, committed, "{takes}");
    }
}
