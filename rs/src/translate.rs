//! `dmog translate`: a document in one of the formats whose grammar
//! packages this command carries, written in any of them.
//!
//! Every format's package exports its translation parts: its manifest's
//! `translate` object (the shapes it reads and writes, the root its render
//! needs, the schema its events carry), its render, and, where it has
//! them, its lift and its embedding, each an alchemy file. This module
//! reads them into alchemy's [`Part`]s, has alchemy's `translate` module
//! compose the route (the source's lift, the root adapters, the
//! embedding, the inferred table, the render), compiles the composition
//! with transduce's routers and render's renderers, and runs it over the
//! input read with the source format's grammar. A package that ships a
//! transducer part (its manifest's `translate.transduce`, the alchemy file
//! whose entry makes the format's JSON events of the parser's rule passes)
//! is read through it: the route is composed over the transducer
//! (`compose_parsed`), and the input's rule passes stream through
//! transduce's `ParserEventSource` into the program, whatever the grammar,
//! with nothing held but what the part keeps, and the engine's arrays
//! emptied behind the stream where transduce verifies the grammar alike
//! pruned and not ([`Format::pruned`]), and the output written as the run
//! goes. Any other format is parsed whole and its value walked, and so is
//! a document when a path selects a value below the root, or when the
//! request asks it read whole ([`Request::whole`], `--whole`). A document
//! whose events are not what the grammar's value holds, which the part
//! refuses part way (a YAML stream of several documents, a merge key, a
//! repeated member, which a part streams twice and `tree-contract`
//! refuses), fails with its code after what it wrote, since the output has
//! left: `--whole` reads it whole, its grammar's value the document (admin
//! ADR-33). Where a package's parser builds a value that
//! is not yet the tree its parts declare, and the package's API reads a
//! document as that tree (expr's simplified form, proto's descriptor, C's
//! realized syntax tree, and the grammar spec a grammar notation's
//! document compiles to), that tree is the document, read whole. Nothing
//! here knows a format by its name: a format is what its manifest says,
//! and a package whose manifest names no parts this host can take is not
//! a format here.
//!
//! A document may be a file ([`Document::File`]) or a stream
//! ([`Document::Reader`], standard input): the transducer part's route
//! reads it through a reader (transduce's `ParserEventSource::from_reader`),
//! a window of it at a time where the source's grammar declares
//! `lex.window` (tabnas/parser's admin ADR-30), and every other route reads
//! it whole.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, OnceLock};

use tabnas_alchemy::translate::{
    compose, compose_parsed, compose_program, compose_program_parsed, Alc, Composition, Descriptor,
    Front, Options, Part, PartText, Render, Root, Shape,
};
use tabnas_alchemy::{Input, Output, Program, Source};
use tabnas_transduce::{
    capability, walk_datum, AbortFlag, Code, Datum, DatumBuilder, Duplicates, Fail, Flow, Guarded,
    JsonEvent, Limits, Metrics, ParserEventSource, ParserSource, Segment, Sink, TreeContract,
};

/// A format's grammar applied to a document, three ways, each over a
/// fresh parser of the grammar under the format's own options, since a
/// source owns its parser: as transduce's source of a text's events
/// ([`ParserSource`]), and as its source of the parser's rule passes
/// ([`ParserEventSource`]) over a text or over what a reader gives, which a
/// program over parser events reads (a transducer part, `run --grammar`).
#[derive(Clone, Copy)]
struct Parsing {
    events: for<'s> fn(&'s str) -> ParserSource<'s>,
    passes: for<'s> fn(&'s str) -> ParserEventSource<'s>,
    reads: fn(Box<dyn Read + Send>) -> ParserEventSource<'static>,
}

/// How a format's documents are read.
#[derive(Clone, Copy)]
enum Reader {
    /// With the grammar's parser.
    Parser(Parsing),
    /// With the grammar's parser, after a check the package's own parse
    /// makes before the parser runs, which the parser does not make
    /// itself: a document the check refuses is not parsed.
    Checked(fn(&str) -> Result<(), Fail>, Parsing),
    /// As the tree the package's own API reads a document as, where its
    /// parser's value is not yet the tree its parts declare: read whole,
    /// and walked as a parser's value is.
    Tree(fn(&str) -> Result<Datum, Fail>),
}

/// One format this command reads and writes: its parts, as its package's
/// manifest names them, the grammar its documents are read with, and the
/// transducer part its package ships, where it ships one.
pub struct Format {
    pub part: Part,
    reader: Reader,
    transducer: Option<Alc>,
    /// The transducer part compiled on its own, once asked for.
    compiled: OnceLock<Result<Option<Program>, Fail>>,
}

impl Format {
    /// The manifest's `languageId`.
    pub fn id(&self) -> &str {
        &self.part.id
    }

    /// The transducer part the format's package ships, its manifest's
    /// `translate.transduce`: the alchemy file whose entry makes the
    /// format's JSON events of its parser's rule passes, through which
    /// `translate` reads the format's documents. `None` where the package
    /// ships none, and the documents are read as [`Format::read`] reads
    /// them.
    pub fn transducer(&self) -> Option<&Alc> {
        self.transducer.as_ref()
    }

    /// Whether the format's documents are parsed, so that a program over
    /// parser events can read them ([`Format::passes`]); a format read as
    /// the tree its package's API makes of a document has no parser here.
    pub fn parses(&self) -> bool {
        !matches!(self.reader, Reader::Tree(_))
    }

    /// The parser's rule passes over `text`: transduce's source of
    /// `ParserEvents/1` with a fresh parser of the format's grammar, for a
    /// program over parser events, a transducer part's or `run --grammar`'s.
    /// The check the package's parse makes first, where it makes one, runs
    /// first, and its refusal is the `Err`; a format read as a tree has no
    /// parser, and refuses.
    pub fn passes<'s>(&self, text: &'s str) -> Result<ParserEventSource<'s>, Fail> {
        match self.reader {
            Reader::Parser(parsing) => Ok((parsing.passes)(text)),
            Reader::Checked(check, parsing) => check(text).map(|()| (parsing.passes)(text)),
            Reader::Tree(_) => Err(self.no_passes()),
        }
    }

    /// The parser's rule passes over what `reader` gives, as
    /// [`Format::passes`] answers them over the same text: read a window at
    /// a time where the grammar declares `lex.window`, and whole first
    /// otherwise. A reader that fails, or bytes that are not UTF-8, end the
    /// run where the parser meets them, with the engine's `unreadable`
    /// ([`unreadable`]). The check the package's parse makes first, where
    /// it makes one, reads the document whole, and the parser then reads
    /// the text it passed.
    pub fn passes_from(
        &self,
        mut reader: Box<dyn Read + Send>,
    ) -> Result<ParserEventSource<'static>, Fail> {
        match self.reader {
            Reader::Parser(parsing) => Ok((parsing.reads)(reader)),
            Reader::Checked(check, parsing) => {
                let mut text = String::new();
                reader.read_to_string(&mut text).map_err(cannot_read)?;
                check(&text)?;
                Ok((parsing.reads)(Box::new(std::io::Cursor::new(
                    text.into_bytes(),
                ))))
            }
            Reader::Tree(_) => Err(self.no_passes()),
        }
    }

    /// `source`, pruned where transduce verifies the format's grammar alike
    /// pruned and not (`capability::prunable`, by the format's id): the
    /// engine's arrays are emptied behind the stream, so its tree holds no
    /// record the stream has passed. Any other grammar's source is left as
    /// it is, since a run that prunes an unlisted grammar is refused before
    /// the parse.
    pub fn pruned<'s>(&self, source: ParserEventSource<'s>) -> ParserEventSource<'s> {
        if capability::prunable(self.id()) {
            source.grammar(self.id()).prune()
        } else {
            source
        }
    }

    /// The refusal of a format read as a tree, which has no rule passes.
    fn no_passes(&self) -> Fail {
        Fail::input(format!(
            "{} is read as a tree, not parsed: it has no rule passes for a program to read",
            self.id()
        ))
    }

    /// The transducer part compiled on its own, over the parser's rule
    /// passes, its entry the export: the stage a host puts in front of a
    /// program of its own, as `run` does in front of a program over a JSON
    /// document, and the one [`Format::read`] reads through. Compiled once,
    /// when first asked for; `None` where the package ships no part.
    pub fn transducer_program(&self) -> Result<Option<Program>, Fail> {
        self.compiled
            .get_or_init(|| {
                let Some(alc) = &self.transducer else {
                    return Ok(None);
                };
                let main = format!("def export [input] ({} input)", alc.entry);
                let sources = [
                    Source::new(&alc.file, &alc.text),
                    Source::new("main", &main),
                ];
                tabnas_alchemy::compile_sources_with(
                    &sources,
                    Input::ParserEvents,
                    Arc::new(tabnas_transduce::routers()),
                    Arc::new(tabnas_render::renderers()),
                )
                .map(Some)
            })
            .clone()
    }

    /// A document read whole with this format's grammar, as a value: its
    /// events as `translate` reads them, through the transducer part its
    /// package ships (so that a number keeps the lexeme the document
    /// spelled it with), collected; or the grammar's own value, walked,
    /// where the package ships no part and where the part's events are
    /// not what the grammar's value holds (a repeated member, which the
    /// part streams twice). A repeated member keeps its last value.
    pub fn read(&self, input: &str, limits: &Limits) -> Result<Datum, Fail> {
        if let Some(part) = self.transducer_program()? {
            match read_through(self, &part, input, limits) {
                Err(fail) if unfollowed(&fail) => {}
                outcome => return outcome,
            }
        }
        read(self, input, limits)
    }
}

/// A package's parts as alchemy's descriptor, with the transducer part its
/// manifest may name beside it: the descriptor carries none, and
/// [`Part::transducer`] reads the two together.
struct Described {
    descriptor: Descriptor,
    transduce: Option<PartText>,
}

/// A package's parts as alchemy's descriptor. Every grammar package owns
/// its interface types, so this reads the same fields from each one's and
/// turns them into alchemy's at once; a package whose parts carry an
/// embedding says so with `embed`, and one whose parts carry a transducer
/// with `transduce`.
macro_rules! descriptor {
    ($package:literal, $module:ident) => {
        $module::translate().map(|parts| Described {
            descriptor: Descriptor {
                package: $package.to_string(),
                manifest: parts.manifest.to_string(),
                lift: parts.lift.map(|p| PartText::new(p.entry, p.source)),
                embed: None,
                render: parts.render.map(|p| PartText::new(p.entry, p.source)),
            },
            transduce: None,
        })
    };
    ($package:literal, $module:ident, embed) => {
        $module::translate().map(|parts| Described {
            descriptor: Descriptor {
                package: $package.to_string(),
                manifest: parts.manifest.to_string(),
                lift: parts.lift.map(|p| PartText::new(p.entry, p.source)),
                embed: parts.embed.map(|p| PartText::new(p.entry, p.source)),
                render: parts.render.map(|p| PartText::new(p.entry, p.source)),
            },
            transduce: None,
        })
    };
    ($package:literal, $module:ident, transduce) => {
        $module::translate().map(|parts| Described {
            descriptor: Descriptor {
                package: $package.to_string(),
                manifest: parts.manifest.to_string(),
                lift: parts.lift.map(|p| PartText::new(p.entry, p.source)),
                embed: None,
                render: parts.render.map(|p| PartText::new(p.entry, p.source)),
            },
            transduce: parts.transduce.map(|p| PartText::new(p.entry, p.source)),
        })
    };
    ($package:literal, $module:ident, embed, transduce) => {
        $module::translate().map(|parts| Described {
            descriptor: Descriptor {
                package: $package.to_string(),
                manifest: parts.manifest.to_string(),
                lift: parts.lift.map(|p| PartText::new(p.entry, p.source)),
                embed: parts.embed.map(|p| PartText::new(p.entry, p.source)),
                render: parts.render.map(|p| PartText::new(p.entry, p.source)),
            },
            transduce: parts.transduce.map(|p| PartText::new(p.entry, p.source)),
        })
    };
}

/// The three sources of a grammar's parser, each over a fresh parser
/// `$make` makes: a text's events, and the parser's rule passes over a text
/// and over a reader.
macro_rules! parsing {
    ($make:expr) => {{
        fn events(text: &str) -> ParserSource<'_> {
            ParserSource::new($make, text)
        }
        fn passes(text: &str) -> ParserEventSource<'_> {
            ParserEventSource::new($make, text)
        }
        fn reads(reader: Box<dyn Read + Send>) -> ParserEventSource<'static> {
            ParserEventSource::from_reader($make, reader)
        }
        Parsing {
            events,
            passes,
            reads,
        }
    }};
}

/// The reader of a package's grammar with its own options.
macro_rules! reader {
    ($module:ident) => {
        Reader::Parser(parsing!($module::make()))
    };
}

/// The reader of chess's grammar with its default options, which its
/// `make` takes: they always install, so a failure is the package's defect.
fn read_pgn() -> Reader {
    Reader::Parser(parsing!(tabnas_chess::make(
        &tabnas_chess::ChessOptions::default()
    )
    .unwrap_or_else(|e| panic!(
        "tabnas-chess: the grammar does not install: {e}"
    ))))
}

/// json5's check before its parser runs, which its `parse_with` makes and
/// its parser does not: a document holding no value is refused as
/// `json5_empty` or `json5_no_value`, rather than as the engine's
/// `unexpected`. Only a document that may hold none, one that is empty or
/// opens, after whitespace, with a comment, is put to the package's parse.
fn json5_value_check(text: &str) -> Result<(), Fail> {
    let rest = text.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    if !(rest.is_empty() || rest.starts_with('/')) {
        return Ok(());
    }
    match tabnas_json5::parse_with(&tabnas_json5::make(), text) {
        Err(error) if error.code == "json5_empty" || error.code == "json5_no_value" => {
            Err(Fail::from_tabnas(&error))
        }
        _ => Ok(()),
    }
}

/// The reader of json5's grammar, run after [`json5_value_check`].
fn read_json5() -> Reader {
    Reader::Checked(json5_value_check, parsing!(tabnas_json5::make()))
}

/// An engine error as transduce's parser source reports one: the input's,
/// with the engine's code and position, and a cancel, which no abort of
/// this command's asks for, a guard of the grammar's own. (The engine's
/// error is the one every grammar re-exports under a name of its own.)
fn engine_failure(error: &tabnas_json::JsonError) -> Fail {
    let mut fail = Fail::from_tabnas(error);
    if error.code == "cancel" {
        fail.message = format!(
            "the grammar stopped the parse with a guard of its own ({}: {}); a grammar \
             may refuse nesting or size below this crate's Limits",
            error.code,
            error.detail.trim_end()
        );
    }
    fail
}

/// An expression as expr's API reads one for its shared fixtures, the tree
/// its parts declare: each operation a list whose first element is the
/// operator's source text (`parse_simplified`). The parser's own value
/// holds each operation as a handle into the parse's arena, which only the
/// package's API reads.
fn read_expr(text: &str) -> Result<Datum, Fail> {
    tabnas_expr::parse_simplified(&tabnas_expr::make(), text)
        .map(|tree| Datum::from_tabnas(&tree))
        .map_err(|error| engine_failure(&error))
}

/// A `.proto` document as proto's API reads one, the tree its parts
/// declare: the FileDescriptorProto the canonical parse returns, every
/// member named and ordered as its object has them (`parse_value`). Its
/// parse makes the check the parser does not, before the engine runs: a
/// document nesting deeper than its cap (`tabnas_proto::MAX_NESTING_DEPTH`)
/// is refused before the engine builds a tree that deep, whose drop could
/// abort the process. The parser's own value is the grammar's syntax tree,
/// which only the package's API reads.
fn read_proto(text: &str) -> Result<Datum, Fail> {
    tabnas_proto::parse_value(text, None)
        .map(|tree| Datum::from_tabnas(&tree))
        .map_err(|error| match error {
            tabnas_proto::ProtoError::Parse(error) => engine_failure(&error),
            refused => Fail::input(refused.to_string()),
        })
}

/// A C translation unit as c's API reads one, the tree its parts declare
/// (`parse`, which realizes the parse into plain values). The parser's own
/// value is a handle into the parse's per-thread arena, which only the
/// package's API reads.
fn read_c(text: &str) -> Result<Datum, Fail> {
    tabnas_c::parse(text)
        .map(|tree| Datum::from_tabnas(&tree))
        .map_err(|error| engine_failure(&error))
}

/// A grammar spec's strict JSON text, which a notation's compiler writes, as
/// the tree it holds, read with the JSON grammar.
fn spec_tree(json: &str) -> Result<Datum, Fail> {
    tabnas_json::parse(json)
        .map(|spec| Datum::from_tabnas(&spec))
        .map_err(|error| engine_failure(&error))
}

/// An ABNF document as abnf's API reads one for a host, the tree its parts
/// declare: the pure-data grammar spec its compiler writes (`abnf_compile`,
/// recognition off, so that the tree builders stay, as strict JSON), read
/// as JSON. The grammar spec is the schema grammar-spec, which the EBNF and
/// GBNF compilers emit too.
fn read_abnf(text: &str) -> Result<Datum, Fail> {
    let options = tabnas_abnf::AbnfCompileOptions {
        start: None,
        tag: None,
        recognition: false,
        strict: true,
        indent: None,
    };
    let compiled = tabnas_abnf::abnf_compile(text, &options)
        .map_err(|error| Fail::input(error.to_string()))?;
    spec_tree(&compiled)
}

/// A notation's conversion, converted with the builtins on, as the
/// shared compiler writes it for a host: the pure-data grammar spec, with
/// its tree builders (`compile_spec` with recognition off, which is
/// `to_jsonic` over `to_pure_spec`), as strict JSON, read as JSON. The
/// compiler is tabnas-bnf, whose serializers tabnas-abnf re-exports, so
/// reading a notation takes no package beyond the notations'.
fn compiled_tree(spec: &tabnas_abnf::GrammarSpec) -> Result<Datum, Fail> {
    let pure = tabnas_abnf::to_pure_spec(spec).map_err(|error| Fail::input(error.to_string()))?;
    let json = tabnas_abnf::to_jsonic(
        &pure,
        tabnas_abnf::JsonicOptions {
            strict: true,
            indent: None,
        },
    );
    spec_tree(&json)
}

/// An EBNF document as ebnf's API reads one for a host, the tree its parts
/// declare: the grammar spec the shared compiler writes of `ebnf_convert`
/// with the builtins on ([`compiled_tree`]).
fn read_ebnf(text: &str) -> Result<Datum, Fail> {
    let convert = tabnas_ebnf::EbnfConvertOptions {
        builtins: true,
        ..tabnas_ebnf::EbnfConvertOptions::default()
    };
    let spec = tabnas_ebnf::ebnf_convert(text, Some(&convert))
        .map_err(|error| Fail::input(error.to_string()))?;
    compiled_tree(&spec)
}

/// A GBNF document as gbnf's API reads one for a host, the tree its parts
/// declare: the grammar spec the shared compiler writes of `gbnf_convert`
/// with the builtins on, and the notation's own defaults otherwise (the
/// start `root`, eager classes where they are unambiguous)
/// ([`compiled_tree`]).
fn read_gbnf(text: &str) -> Result<Datum, Fail> {
    let convert = tabnas_gbnf::GbnfConvertOptions::new(tabnas_gbnf::ConvertOptions {
        builtins: true,
        ..tabnas_gbnf::ConvertOptions::default()
    });
    let spec = tabnas_gbnf::gbnf_convert(text, Some(&convert))
        .map_err(|error| Fail::input(error.to_string()))?;
    compiled_tree(&spec)
}

/// Every grammar package this command carries, with its reader.
fn packages() -> Vec<(Option<Described>, Reader)> {
    vec![
        (
            descriptor!("tabnas-abnf", tabnas_abnf, embed),
            Reader::Tree(read_abnf),
        ),
        (
            descriptor!("tabnas-c", tabnas_c, embed),
            Reader::Tree(read_c),
        ),
        (
            descriptor!("tabnas-chess", tabnas_chess, embed, transduce),
            read_pgn(),
        ),
        (
            descriptor!("tabnas-css", tabnas_css, embed, transduce),
            reader!(tabnas_css),
        ),
        (
            descriptor!("tabnas-csv", tabnas_csv, embed, transduce),
            reader!(tabnas_csv),
        ),
        (
            descriptor!("tabnas-ebnf", tabnas_ebnf, embed),
            Reader::Tree(read_ebnf),
        ),
        (
            descriptor!("tabnas-expr", tabnas_expr, embed),
            Reader::Tree(read_expr),
        ),
        (
            descriptor!("tabnas-feed", tabnas_feed, embed),
            reader!(tabnas_feed),
        ),
        (
            descriptor!("tabnas-gbnf", tabnas_gbnf, embed),
            Reader::Tree(read_gbnf),
        ),
        (
            descriptor!("tabnas-ini", tabnas_ini, embed, transduce),
            reader!(tabnas_ini),
        ),
        (
            descriptor!("tabnas-json", tabnas_json, embed, transduce),
            reader!(tabnas_json),
        ),
        (
            descriptor!("tabnas-json5", tabnas_json5, embed, transduce),
            read_json5(),
        ),
        (
            descriptor!("tabnas-jsonc", tabnas_jsonc, embed, transduce),
            reader!(tabnas_jsonc),
        ),
        (
            descriptor!("tabnas-jsonic", tabnas_jsonic, embed, transduce),
            reader!(tabnas_jsonic),
        ),
        (
            descriptor!("tabnas-jsonl", tabnas_jsonl, embed, transduce),
            reader!(tabnas_jsonl),
        ),
        (
            descriptor!("tabnas-markdown", tabnas_markdown, embed),
            reader!(tabnas_markdown),
        ),
        (
            descriptor!("tabnas-proto", tabnas_proto, embed),
            Reader::Tree(read_proto),
        ),
        (
            descriptor!("tabnas-semver", tabnas_semver, embed),
            reader!(tabnas_semver),
        ),
        (
            descriptor!("tabnas-toml", tabnas_toml, embed, transduce),
            reader!(tabnas_toml),
        ),
        (
            descriptor!("tabnas-xml", tabnas_xml, embed, transduce),
            reader!(tabnas_xml),
        ),
        (
            descriptor!("tabnas-yaml", tabnas_yaml, transduce),
            reader!(tabnas_yaml),
        ),
        (
            descriptor!("tabnas-zon", tabnas_zon, transduce),
            reader!(tabnas_zon),
        ),
    ]
}

/// Every format, by id, read once.
pub fn formats() -> &'static [Format] {
    static FORMATS: OnceLock<Vec<Format>> = OnceLock::new();
    FORMATS.get_or_init(|| {
        let mut formats: Vec<Format> = packages()
            .into_iter()
            .filter_map(|(described, reader)| {
                let Described {
                    descriptor,
                    transduce,
                } = described?;
                let part = Part::from_descriptor(&descriptor)?;
                // A transducer the manifest and the package disagree on is
                // none here; the translation-parts test holds the two
                // together.
                let transducer = Part::transducer(&descriptor, transduce.as_ref()).flatten();
                Some(Format {
                    part,
                    reader,
                    transducer,
                    compiled: OnceLock::new(),
                })
            })
            .collect();
        formats.sort_by(|a, b| a.part.id.cmp(&b.part.id));
        formats
    })
}

/// The format a command line names, by its id.
pub fn format(id: &str) -> Option<&'static Format> {
    formats().iter().find(|f| f.id() == id)
}

/// The formats' ids for a message: `csv, ini, ... or zon`.
pub fn names() -> String {
    names_of(formats().iter().map(Format::id))
}

/// The ids of the formats whose documents are parsed, for a message, as
/// [`names`] lists every format: what `run --grammar` takes.
pub fn parser_names() -> String {
    names_of(formats().iter().filter(|f| f.parses()).map(Format::id))
}

fn names_of<'a>(ids: impl Iterator<Item = &'a str>) -> String {
    let ids: Vec<&str> = ids.collect();
    match ids.split_last() {
        Some((last, [])) => last.to_string(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// What a translation was asked.
pub struct Request<'a> {
    /// The format the input is read with.
    pub from: &'a Format,
    /// The format it is written in.
    pub to: &'a Format,
    /// A value below the root to translate instead of the whole document,
    /// as segments: a plain tree, whatever the source's shapes.
    pub path: Option<Vec<Segment>>,
    /// What the host chooses: the key a root is wrapped under.
    pub options: Options,
    /// A program, as its file name and its text, whose export stands in
    /// the source's place: its JSON events a tree, its table records.
    pub program: Option<(&'a str, &'a str)>,
    /// Whether the document is read whole, as its grammar's value, rather
    /// than streamed through its format's transducer part (`--whole`): the
    /// route for a document the part refuses part way.
    pub whole: bool,
    pub limits: Limits,
}

/// What a request compiles to: the composition over the source's events,
/// the route every format has, and, where the source ships a transducer
/// part and the whole document is asked, not read whole, the composition
/// over its parser's rule passes, which [`run_compiled`] then runs in its
/// place.
pub struct Compiled {
    pub composition: Composition,
    pub program: Program,
    parsed: Option<(Composition, Program)>,
}

/// The composition a request makes, and the program it compiles to.
pub fn compile(request: &Request<'_>) -> Result<Compiled, Fail> {
    let routers = Arc::new(tabnas_transduce::routers());
    let renderers = Arc::new(tabnas_render::renderers());
    // The transducer the route reads the document through, where the
    // source ships one and the whole document is asked, not read whole: a
    // path selects a value below the root, which only the document's value
    // has.
    let transducer = match (&request.path, request.whole) {
        (None, false) => request.from.transducer(),
        _ => None,
    };
    let (composition, program, parsed) = match request.program {
        Some((file, text)) => {
            let output =
                tabnas_alchemy::compile(text, file, routers.clone(), renderers.clone())?.output();
            if output == Output::Text {
                return Err(Fail::new(
                    Code::DslTypeError,
                    format!(
                        "bad_output: {file}'s export writes its own text, and translate takes a \
                         program whose export answers JSON events or a table, which a format's \
                         render then writes"
                    ),
                ));
            }
            let composition =
                compose_program(output, &request.to.part, &request.options, "translate")?;
            let program = composition.compile(
                Some(Source::new(file, text)),
                routers.clone(),
                renderers.clone(),
            )?;
            let parsed = match transducer {
                Some(alc) => {
                    let composition = compose_program_parsed(
                        output,
                        alc,
                        &request.to.part,
                        &request.options,
                        "translate",
                    )?;
                    let program =
                        composition.compile(Some(Source::new(file, text)), routers, renderers)?;
                    Some((composition, program))
                }
                None => None,
            };
            (composition, program, parsed)
        }
        None => {
            let source = match request.path {
                Some(_) => None,
                None => Some(&request.from.part),
            };
            let composition = compose(source, &request.to.part, &request.options, "translate")?;
            let program = composition.compile(None, routers.clone(), renderers.clone())?;
            let parsed = match transducer {
                Some(alc) => {
                    let composition = compose_parsed(
                        &request.from.part,
                        alc,
                        &request.to.part,
                        &request.options,
                        "translate",
                    )?;
                    let program = composition.compile(None, routers, renderers)?;
                    Some((composition, program))
                }
                None => None,
            };
            (composition, program, parsed)
        }
    };
    Ok(Compiled {
        composition,
        program,
        parsed,
    })
}

/// Whether a transducer part's route refused a document because the
/// grammar's value is not what the passes showed: the part's own refusal
/// (`fail :unstreamable`), a second root, or a repeated member, which the
/// part streams twice and the composition's policy refuses, since the
/// grammar's value may keep the last or merge the two. The output streams
/// as the run goes, so such a document is not read again: it fails with its
/// code, and `--whole` ([`Request::whole`]) reads it whole, as its
/// grammar's value (admin ADR-33).
fn unfollowed(fail: &Fail) -> bool {
    matches!(
        fail.code,
        Code::StreamabilityUnknown | Code::DuplicateMember
    )
}

/// What a refusal of the transducer part's route adds to its message, where
/// no program stands in the source's place, whose own events could make
/// such a refusal: the route that reads the document.
const WHOLE: &str = "; --whole reads the document whole";

/// What a translation reads.
pub enum Document<'a> {
    /// A text, read whole.
    Text(&'a str),
    /// A file. The transducer part's route reads it through a reader
    /// ([`Format::passes_from`]), a window of it at a time where the
    /// source's grammar declares `lex.window`, so that the run holds a
    /// window of the document rather than all of it. Every other route
    /// reads the file whole.
    File(&'a Path),
    /// A stream, such as standard input, read once: through a reader on the
    /// transducer part's route, as a file is, and whole on every other.
    Reader(Box<dyn Read + Send>),
}

#[cfg(test)]
thread_local! {
    /// The files the transducer part's route read through a reader on this
    /// thread, for the tests that hold a file to that route.
    static READ_THROUGH_A_READER: std::cell::RefCell<Vec<std::path::PathBuf>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Run a request over `input`, writing the document to `out` as the run
/// goes; `metrics` collects what the stages report, the output's bytes
/// among it.
pub fn run(
    request: &Request<'_>,
    input: &str,
    out: Box<dyn Write + Send>,
    metrics: Arc<Metrics>,
) -> Result<(), Fail> {
    run_compiled(request, &compile(request)?, input, out, metrics)
}

/// [`run`], with the request's composition compiled already
/// ([`compile`]), so that one compiled composition serves many inputs.
pub fn run_compiled(
    request: &Request<'_>,
    compiled: &Compiled,
    input: &str,
    out: Box<dyn Write + Send>,
    metrics: Arc<Metrics>,
) -> Result<(), Fail> {
    run_document(request, compiled, Document::Text(input), out, metrics)
}

/// [`run_compiled`] over a document, which may be a file or a stream
/// ([`Document`]). A document that cannot be read fails as one a reader
/// gives up on part way does, with the engine's `unreadable`
/// ([`unreadable`]). A failure after bytes have left says so
/// (`committed_output`): the source's failure (bad input after the rows)
/// knows nothing of the output, and the writer's count does.
pub fn run_document(
    request: &Request<'_>,
    compiled: &Compiled,
    document: Document<'_>,
    out: Box<dyn Write + Send>,
    metrics: Arc<Metrics>,
) -> Result<(), Fail> {
    run_once(request, compiled, document, out, metrics.clone()).map_err(|fail| {
        if Metrics::get(&metrics.output_bytes) > 0 && !fail.committed_output {
            fail.committed()
        } else {
            fail
        }
    })
}

/// [`run_document`]'s one run: through the transducer part where the
/// source ships one and the whole document is asked, and over the
/// document's value otherwise.
fn run_once(
    request: &Request<'_>,
    compiled: &Compiled,
    document: Document<'_>,
    out: Box<dyn Write + Send>,
    metrics: Arc<Metrics>,
) -> Result<(), Fail> {
    let Compiled {
        composition,
        program,
        parsed,
    } = compiled;
    if let Some((_, part)) = parsed {
        // The transducer's route: the document's rule passes through the
        // part into the program, streamed whatever the grammar, from a file
        // or a stream through a reader.
        let sink = part.parser_sink(out, None, &request.limits, metrics.clone())?;
        let passes = match document {
            Document::Text(text) => request.from.passes(text),
            Document::File(path) => {
                #[cfg(test)]
                READ_THROUGH_A_READER.with(|read| read.borrow_mut().push(path.into()));
                std::fs::File::open(path)
                    .map_err(cannot_read)
                    .and_then(|file| request.from.passes_from(Box::new(file)))
            }
            Document::Reader(reader) => request.from.passes_from(reader),
        }?;
        return request
            .from
            .pruned(passes)
            .limits(request.limits.clone())
            .metrics(metrics)
            .run_boxed(sink)
            .0
            .map(|_| ())
            .map_err(|mut fail| {
                if unfollowed(&fail) && request.program.is_none() {
                    fail.message.push_str(WHOLE);
                }
                fail
            });
    }
    let owned;
    let text = match document {
        Document::Text(text) => text,
        Document::File(path) => {
            owned = read_file(path)?;
            &owned
        }
        Document::Reader(mut reader) => {
            let mut whole = String::new();
            reader.read_to_string(&mut whole).map_err(cannot_read)?;
            owned = whole;
            &owned
        }
    };
    let sink = program.sink(out, None, &request.limits, metrics.clone())?;
    // The source's events reach a tree's render as the source made them,
    // so a stream that is no tree's is refused in front of it rather than
    // written half way.
    let mut sink: Box<dyn Sink + Send> = match composition.front {
        Front::Tree => Box::new(TreeContract::new(sink)),
        _ => sink,
    };
    match &request.path {
        None => {
            source(request.from, text, &request.limits, metrics, sink).0?;
        }
        Some(path) => {
            let value = read(request.from, text, &request.limits)?;
            let selected = value.get_path(path).ok_or_else(|| {
                Fail::input(format!(
                    "the path {} names nothing in the document",
                    path_text(path)
                ))
            })?;
            if walk_datum(selected, &mut sink)? == Flow::Continue {
                sink.event(JsonEvent::End)?;
            }
        }
    }
    Ok(())
}

/// A file's text, read whole. A file that cannot be read, or that holds
/// bytes that are not UTF-8, fails as a reader that fails does
/// ([`cannot_read`]).
fn read_file(path: &Path) -> Result<String, Fail> {
    std::fs::read_to_string(path).map_err(cannot_read)
}

/// A document that could not be read, failed as the engine's `unreadable`
/// fails a parse from a reader once transduce reports it: `INPUT_INVALID`,
/// the code and why ([`unreadable`] reads it back).
fn cannot_read(error: std::io::Error) -> Fail {
    Fail::input(format!("unreadable: {error}"))
}

/// Why a run's document could not be read, where that is why it failed: a
/// read that failed, or bytes that are not UTF-8, which a parse from a
/// reader meets where they are (the engine's `unreadable`, which transduce
/// reports as `INPUT_INVALID`, the engine's code before its message), and
/// a file read whole that could not be. A host reports such a document as
/// a file it could not read, not as one that does not parse.
pub fn unreadable(fail: &Fail) -> Option<&str> {
    if fail.code != Code::InputInvalid {
        return None;
    }
    let reason = fail.message.strip_prefix("unreadable: ")?;
    Some(
        reason
            .strip_prefix("the source could not be read: ")
            .unwrap_or(reason),
    )
}

/// The events of a document, collected into a value, which keeps the last
/// of a repeated member; shared, so that a sink handed on by value leaves
/// the value where its caller takes it.
#[derive(Clone)]
struct Collect(Arc<std::sync::Mutex<DatumBuilder>>);

impl Collect {
    fn new() -> Collect {
        Collect(Arc::new(std::sync::Mutex::new(DatumBuilder::new(
            usize::MAX,
            "max_capture_bytes",
            Duplicates::LastWins,
        ))))
    }

    fn builder(&self) -> std::sync::MutexGuard<'_, DatumBuilder> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The value collected, once the document has ended.
    fn take(&self) -> Result<Datum, Fail> {
        self.builder()
            .take()
            .ok_or_else(|| Fail::input("the document holds no value"))
    }
}

impl Sink for Collect {
    fn event(&mut self, ev: JsonEvent<'_>) -> Result<Flow, Fail> {
        if !matches!(ev, JsonEvent::End) {
            self.builder().event(ev)?;
        }
        Ok(Flow::Continue)
    }
}

/// A document's events as a format reads it whole, into `sink`, which
/// comes back with the outcome: its parser's value, walked by transduce's
/// parser source (after the check its package's parse makes first, where
/// it makes one), or the tree its package's API reads, walked under the
/// same limits and counts.
fn source<S: Sink + Send + 'static>(
    format: &Format,
    input: &str,
    limits: &Limits,
    metrics: Arc<Metrics>,
    sink: S,
) -> (Result<Flow, Fail>, S) {
    match format.reader {
        Reader::Parser(parsing) => (parsing.events)(input)
            .limits(limits.clone())
            .metrics(metrics)
            .run_owned(sink),
        Reader::Checked(check, parsing) => match check(input) {
            Err(refused) => (Err(refused), sink),
            Ok(()) => (parsing.events)(input)
                .limits(limits.clone())
                .metrics(metrics)
                .run_owned(sink),
        },
        Reader::Tree(read) => {
            let mut guarded = Guarded::new(sink, limits, AbortFlag::new(), metrics);
            let outcome = read(input).and_then(|tree| match walk_datum(&tree, &mut guarded)? {
                Flow::Continue => guarded.event(JsonEvent::End),
                Flow::Stop => Ok(Flow::Stop),
            });
            (outcome, guarded.into_inner())
        }
    }
}

/// A document read whole with a format's grammar, as a value. A repeated
/// member keeps its last value, as the grammar's own value does.
fn read(format: &Format, input: &str, limits: &Limits) -> Result<Datum, Fail> {
    let (outcome, collect) = source(format, input, limits, Metrics::new(), Collect::new());
    outcome?;
    collect.take()
}

/// A document read through a format's transducer part, as a value: the
/// part's events over the parser's rule passes, held to a tree's contract
/// (one root, each key once in an object) and collected.
fn read_through(
    format: &Format,
    part: &Program,
    input: &str,
    limits: &Limits,
) -> Result<Datum, Fail> {
    let collect = Collect::new();
    let metrics = Metrics::new();
    let head = part.parser_sink_into(
        Box::new(TreeContract::new(collect.clone())),
        limits,
        metrics.clone(),
    )?;
    format
        .passes(input)?
        .limits(limits.clone())
        .metrics(metrics)
        .run_boxed(head)
        .0?;
    collect.take()
}

/// A path given as a JSON array of segments, each a string (a member's
/// key) or a whole number (an element's index), the form alchemy's
/// `as-path` validates.
pub fn parse_path(text: &str, limits: &Limits) -> Result<Vec<Segment>, Fail> {
    let json = format("json").ok_or_else(|| Fail::input("no JSON grammar to read the path"))?;
    let refuse = || {
        Fail::input(format!(
            "--path takes a JSON array of keys and indexes, such as [\"people\",0], not {text}"
        ))
    };
    let value = json.read(text, limits).map_err(|_| refuse())?;
    let Some(items) = value.as_array() else {
        return Err(refuse());
    };
    items
        .iter()
        .map(|item| match item {
            Datum::String(key) => Ok(Segment::Key(key.clone())),
            Datum::Number { value, .. }
                if value.fract() == 0.0 && *value >= 0.0 && *value <= usize::MAX as f64 =>
            {
                Ok(Segment::Index(*value as usize))
            }
            _ => Err(refuse()),
        })
        .collect()
}

/// A path as the JSON array it was given as.
fn path_text(path: &[Segment]) -> String {
    let items: Vec<Datum> = path
        .iter()
        .map(|s| match s {
            Segment::Key(k) => Datum::String(k.clone()),
            Segment::Index(i) => Datum::Number {
                value: *i as f64,
                lexeme: Some(i.to_string().into()),
            },
        })
        .collect();
    Datum::Array(items).to_string()
}

/// The registry as JSON, one object per format, for `dmog formats`:
/// its id, the shapes it reads and writes, the root its render needs, its
/// schema, why its documents are read whole where it says (`whole`, null
/// where it does not), its parts' entries, and its loss sentences.
pub fn formats_json() -> String {
    let shape = |s: &Shape| {
        Datum::String(
            match s {
                Shape::Tree => "tree",
                Shape::Records => "records",
            }
            .into(),
        )
    };
    let text = |s: &str| Datum::String(s.into());
    let items: Vec<Datum> = formats()
        .iter()
        .map(|f| {
            let p = &f.part;
            let fields: [(&str, Datum); 11] = [
                ("id", text(&p.id)),
                ("reads", Datum::Array(p.reads.iter().map(shape).collect())),
                ("writes", shape(&p.writes)),
                (
                    "root",
                    text(match p.root {
                        Root::Object => "object",
                        Root::Array => "array",
                        Root::Any => "any",
                    }),
                ),
                ("schema", p.schema.as_deref().map_or(Datum::Null, text)),
                ("whole", p.whole.as_deref().map_or(Datum::Null, text)),
                (
                    "lift",
                    p.lift.as_ref().map_or(Datum::Null, |a| text(&a.entry)),
                ),
                (
                    "embed",
                    p.embed.as_ref().map_or(Datum::Null, |a| text(&a.entry)),
                ),
                (
                    "render",
                    text(match &p.render {
                        Render::Alc(a) => &a.entry,
                        Render::Json => "json",
                        Render::Csv => "csv",
                    }),
                ),
                (
                    "transduce",
                    f.transducer
                        .as_ref()
                        .map_or(Datum::Null, |a| text(&a.entry)),
                ),
                (
                    "loss",
                    Datum::Array(p.loss.iter().map(|l| text(l)).collect()),
                ),
            ];
            Datum::Object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
        })
        .collect();
    Datum::Array(items).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a run writes, held for the test to read.
    #[derive(Clone, Default)]
    struct Held(Arc<std::sync::Mutex<Vec<u8>>>);

    impl Write for Held {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// A transducer that writes `{\"ran\":true}` whatever the document:
    /// what it writes says which route ran.
    const MARKING_TRANSDUCE: &str = "def json-transduce [input]\n  as-events\n    scan-emit\n      []\n      fn [s pass] (transition s [])\n      fn [s] [object-start (key \"ran\") (scalar true) object-end]\n      parser-events input\n";

    /// json without its transducer: read as a format whose package ships
    /// none is read.
    fn plain_json() -> Format {
        let json = format("json").expect("json is a format");
        Format {
            part: json.part.clone(),
            reader: json.reader,
            transducer: None,
            compiled: OnceLock::new(),
        }
    }

    /// json, read through a transducer part given beside its package's
    /// parts in place of the one it ships.
    fn transduced_json(text: &str) -> Format {
        let json = format("json").expect("json is a format");
        Format {
            part: json.part.clone(),
            reader: json.reader,
            transducer: Some(Alc {
                file: "tabnas-json/alchemy/transduce.alc".to_string(),
                entry: "json-transduce".to_string(),
                text: text.to_string(),
            }),
            compiled: OnceLock::new(),
        }
    }

    fn translated(
        from: &Format,
        to: &str,
        path: Option<&str>,
        program: Option<(&str, &str)>,
        input: &str,
    ) -> Result<String, Fail> {
        translated_as(from, to, path, program, input, false)
    }

    /// [`translated`], read whole ([`Request::whole`]) where `whole` is
    /// asked.
    fn translated_as(
        from: &Format,
        to: &str,
        path: Option<&str>,
        program: Option<(&str, &str)>,
        input: &str,
        whole: bool,
    ) -> Result<String, Fail> {
        let limits = Limits::default();
        let request = Request {
            from,
            to: format(to).expect("a format"),
            path: path.map(|p| parse_path(p, &limits).expect("a path")),
            options: Options::default(),
            program,
            whole,
            limits,
        };
        let held = Held::default();
        run(&request, input, Box::new(held.clone()), Metrics::new())?;
        let bytes = held.0.lock().unwrap().clone();
        Ok(String::from_utf8(bytes).expect("the output is UTF-8"))
    }

    /// The formats read through a transducer part are those whose packages
    /// ship one and the registry reads (the `transduce` field of the
    /// package's descriptor): each under the entry its manifest names, and
    /// no other.
    #[test]
    fn the_formats_read_through_a_transducer_part() {
        let through: Vec<(&str, &str)> = formats()
            .iter()
            .filter_map(|f| f.transducer().map(|alc| (f.id(), alc.entry.as_str())))
            .collect();
        assert_eq!(
            through,
            [
                ("css", "css-transduce"),
                ("csv", "csv-transduce"),
                ("ini", "ini-transduce"),
                ("json", "json-transduce"),
                ("json5", "json5-transduce"),
                ("jsonc", "jsonc-transduce"),
                ("jsonic", "jsonic-transduce"),
                ("jsonl", "jsonl-transduce"),
                ("pgn", "pgn-transduce"),
                ("toml", "toml-transduce"),
                ("xml", "xml-transduce"),
                ("yaml", "yaml-transduce"),
                ("zon", "zon-transduce"),
            ]
        );
    }

    /// A format whose package ships a transducer part is read through it
    /// (json, through tabnas-json's), its rule passes streamed into the
    /// composition over the part, and one without is read whole, its
    /// value walked: the two routes write the same bytes for the same
    /// document, into a tree target, a records target, and with a program
    /// over the input, but for a number's lexeme, which the part keeps and
    /// the walk spells as the engine does; a marking part given in the
    /// package's place shows the route ran. A path keeps the document's
    /// value, the one route a path has. A repeated member, which the part
    /// streams twice and the composition's tree-contract refuses, fails
    /// with its code and names `--whole`, since the output streams and is
    /// not written twice; read whole, it is the grammar's value, as a format
    /// without a part writes it (admin ADR-33).
    #[test]
    fn a_transducer_part_reads_the_format_and_the_walk_stands_in_where_it_is_asked() {
        let plain = plain_json();
        let through = format("json").expect("json is a format");
        let marking = transduced_json(MARKING_TRANSDUCE);
        assert!(plain.transducer().is_none());
        assert_eq!(
            through.transducer().map(|alc| alc.entry.as_str()),
            Some("json-transduce"),
            "the registry's json is read through its package's part"
        );
        assert!(through
            .transducer()
            .is_some_and(|alc| alc.text.contains("transduce-tree")));
        let tree = r#"{"a":[1,2.50,"x",{"b":null}],"c":{},"d":[],"e":1e21,"f":true}"#;
        // The same document, its numbers spelled as the engine spells them.
        let spelled = tree.replace("2.50", "2.5");
        assert_eq!(
            translated(through, "json", None, None, tree).unwrap(),
            format!("{tree}\n"),
            "the part keeps a number's lexeme"
        );
        assert_eq!(
            translated(&plain, "json", None, None, tree).unwrap(),
            format!("{spelled}\n"),
            "the walk spells a number as the engine does"
        );
        let records = r#"[{"a":1,"b":"x"},{"a":2,"b":"y"}]"#;
        let echo = Some(("echo.alc", "def export [input] (events input)\n"));
        for (to, doc, program) in [
            ("json", spelled.as_str(), None),
            ("yaml", spelled.as_str(), None),
            ("csv", records, None),
            ("json", spelled.as_str(), echo),
            ("csv", records, echo),
        ] {
            let expected =
                translated(&plain, to, None, program, doc).expect("the plain route writes");
            let got =
                translated(through, to, None, program, doc).expect("the transducer's route writes");
            assert_eq!(got, expected, "{to} from {doc} with {program:?}");
            let marked = translated(&marking, to, None, program, doc);
            assert_ne!(
                marked.as_deref().ok(),
                Some(expected.as_str()),
                "{to} from {doc} with {program:?}: the part's route ran"
            );
        }
        assert_eq!(
            translated(&marking, "json", None, None, tree).unwrap(),
            "{\"ran\":true}\n"
        );
        // A path selects in the document's value, read whole, as it did:
        // the materialized value spells a number as the engine does.
        assert_eq!(
            translated(&marking, "json", Some("[\"a\",1]"), None, tree).unwrap(),
            "2.5\n",
            "a path keeps the document's value"
        );
        let repeated = r#"{"a":1,"a":2}"#;
        let streamed = translated(through, "json", None, None, repeated).unwrap_err();
        assert_eq!(streamed.code, Code::DuplicateMember, "{streamed:?}");
        assert!(streamed.message.ends_with(WHOLE), "{streamed:?}");
        assert_eq!(
            translated_as(through, "json", None, None, repeated, true).unwrap(),
            translated(&plain, "json", None, None, repeated).unwrap()
        );
        assert_eq!(
            translated_as(through, "json", None, None, repeated, true).unwrap(),
            "{\"a\":2}\n"
        );
        let refused = translated(through, "json", None, None, "[1,").unwrap_err();
        assert_eq!(refused.code, Code::InputInvalid);
    }

    /// YAML is read through its part, and a repeated key, which the part
    /// streams twice and the tree contract refuses, fails unless the
    /// document is read whole: the grammar merges it, at the root and in a
    /// sequence entry's mapping alike (tabnas-yaml 0.5.26 merges the
    /// entry's, which kept the later value before). A sequence entry's keys
    /// stand at the column its content starts at, so a key with an empty
    /// value before a sibling is null, and an entry with more than one
    /// space after its dash reads (both 0.5.26).
    #[test]
    fn yaml_reads_a_sequence_entry_as_its_grammar_does() {
        let yaml = format("yaml").expect("yaml is a format");
        for (doc, want, repeats) in [
            (
                "a:\n  b: 1\na:\n  c: 2\n",
                "{\"a\":{\"b\":1,\"c\":2}}\n",
                true,
            ),
            (
                "- a:\n    b: 1\n  a:\n    c: 2\n",
                "[{\"a\":{\"b\":1,\"c\":2}}]\n",
                true,
            ),
            ("- a:\n  b: 1\n", "[{\"a\":null,\"b\":1}]\n", false),
            ("-   a: 1\n    b: 2\n", "[{\"a\":1,\"b\":2}]\n", false),
        ] {
            if repeats {
                let streamed = translated(yaml, "json", None, None, doc).unwrap_err();
                assert_eq!(streamed.code, Code::DuplicateMember, "{doc:?}");
            }
            assert_eq!(
                translated_as(yaml, "json", None, None, doc, repeats).unwrap(),
                want,
                "{doc:?}"
            );
        }
    }

    /// `bytes` in a file of its own in the temporary directory.
    fn file_of(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let file =
            std::env::temp_dir().join(format!("dmog-translate-{}-{name}", std::process::id()));
        std::fs::write(&file, bytes).expect("the file is written");
        file
    }

    /// [`translated`], from a file ([`Document::File`]).
    fn translated_file(
        from: &Format,
        to: &str,
        path: Option<&str>,
        file: &Path,
    ) -> Result<String, Fail> {
        translated_file_as(from, to, path, file, false)
    }

    /// [`translated_file`], read whole ([`Request::whole`]) where `whole`
    /// is asked.
    fn translated_file_as(
        from: &Format,
        to: &str,
        path: Option<&str>,
        file: &Path,
        whole: bool,
    ) -> Result<String, Fail> {
        let limits = Limits::default();
        let request = Request {
            from,
            to: format(to).expect("a format"),
            path: path.map(|p| parse_path(p, &limits).expect("a path")),
            options: Options::default(),
            program: None,
            whole,
            limits,
        };
        let held = Held::default();
        run_document(
            &request,
            &compile(&request)?,
            Document::File(file),
            Box::new(held.clone()),
            Metrics::new(),
        )?;
        let bytes = held.0.lock().unwrap().clone();
        Ok(String::from_utf8(bytes).expect("the output is UTF-8"))
    }

    /// Whether the transducer part's route read `file` through a reader.
    fn read_through_a_reader(file: &Path) -> bool {
        READ_THROUGH_A_READER.with(|read| read.borrow().iter().any(|p| p == file))
    }

    /// A file translates as its text does. The transducer part's route
    /// reads it through a reader where the source ships a part and the
    /// whole document is asked, and a document the part refuses (a
    /// repeated member) fails as its text does, as do a byte-order mark, a
    /// document json5's check refuses and one that does not parse. A path,
    /// and a format whose package ships no part, read the file whole.
    #[test]
    fn a_file_translates_as_its_text_does() {
        let cases: &[(&str, &str, Option<&str>, &str)] = &[
            (
                "json",
                "json",
                None,
                r#"{"a":[1,2.50,"x",{"b":null}],"c":{}}"#,
            ),
            ("json", "yaml", None, r#"[{"a":1},{"a":2}]"#),
            ("json", "csv", None, r#"[{"a":1,"b":"x"},{"a":2,"b":"y"}]"#),
            ("json", "json", None, r#"{"a":1,"a":2}"#),
            ("json", "json", None, "\u{feff}[1, 2]"),
            ("json", "json", None, "[1,"),
            ("json", "json", Some("[\"a\",1]"), r#"{"a":[1,2.50]}"#),
            (
                "jsonc",
                "json",
                None,
                "// rows\n[{\"a\": 1, /* c */ \"b\": 2,}]",
            ),
            ("jsonl", "json", None, "{\"a\":1}\n{\"a\":2}\n"),
            ("csv", "json", None, "a,b\n1,\"x\"\n2,y\n"),
            ("json5", "json", None, "{a: 1, b: [2,],}"),
            ("json5", "json", None, "// only a comment"),
            ("yaml", "json", None, "a:\n  b: 1\na:\n  c: 2\n"),
            ("xml", "json", None, "<a><b>1</b></a>"),
            ("toml", "json", None, "a = 1\n[b]\nc = 2\n"),
        ];
        for (index, (from, to, path, text)) in cases.iter().enumerate() {
            let source = format(from).expect("a format");
            let file = file_of(&format!("{index}.{from}"), text.as_bytes());
            let whole = translated(source, to, *path, None, text);
            let read = translated_file(source, to, *path, &file);
            assert_eq!(
                format!("{read:?}"),
                format!("{whole:?}"),
                "{from} {text:?} to {to} at {path:?}"
            );
            assert_eq!(
                read_through_a_reader(&file),
                path.is_none() && source.transducer().is_some(),
                "{from} {text:?} at {path:?}: the route"
            );
            let _ = std::fs::remove_file(&file);
        }
    }

    /// A file that cannot be read fails with the engine's `unreadable` on
    /// every route: bytes that are not UTF-8, which the part's route meets
    /// through a reader and every other route reading the file whole, and a
    /// file that is not there. [`unreadable`] gives the reason as
    /// `read_to_string` words it, and nothing for a document that does not
    /// parse.
    #[test]
    fn a_file_that_cannot_be_read_is_unreadable_on_every_route() {
        let bad = file_of("bad.json", b"[1, \"\xff\"]");
        let json = format("json").expect("json is a format");
        let toml = format("toml").expect("toml is a format");
        for (from, path) in [(json, None), (json, Some("[0]")), (toml, None)] {
            let fail = translated_file(from, "json", path, &bad).unwrap_err();
            assert_eq!(fail.code, Code::InputInvalid, "{fail:?}");
            assert_eq!(
                unreadable(&fail),
                Some("stream did not contain valid UTF-8"),
                "{} at {path:?}: {fail:?}",
                from.id()
            );
        }
        assert!(read_through_a_reader(&bad));
        let _ = std::fs::remove_file(&bad);
        let missing = std::env::temp_dir().join("dmog-translate-no-such-file.json");
        let fail = translated_file(json, "json", None, &missing).unwrap_err();
        let reason = std::fs::read_to_string(&missing).unwrap_err().to_string();
        assert_eq!(unreadable(&fail), Some(reason.as_str()), "{fail:?}");
        let fail = translated(json, "json", None, None, "[1,").unwrap_err();
        assert_eq!(unreadable(&fail), None, "{fail:?}");
    }

    /// A file that is not a regular file, a pipe, is read once, through a
    /// reader as a regular file is, since nothing reads a document again: a
    /// pipe holding a repeated member fails as its text does, and read
    /// whole it translates as its text does.
    #[cfg(unix)]
    #[test]
    fn a_pipe_is_read_once() {
        let json = format("json").expect("json is a format");
        let repeated = r#"{"a":1,"a":2}"#;
        for whole in [false, true] {
            // A pipe of its own for each run, since the record of the files
            // read through a reader is the thread's.
            let fifo = std::env::temp_dir().join(format!(
                "dmog-translate-{}-pipe-{whole}.json",
                std::process::id()
            ));
            let _ = std::fs::remove_file(&fifo);
            let made = std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .expect("mkfifo runs");
            assert!(made.success());
            let writer = {
                let fifo = fifo.clone();
                std::thread::spawn(move || std::fs::write(&fifo, repeated))
            };
            let read = translated_file_as(json, "json", None, &fifo, whole);
            writer.join().unwrap().expect("the pipe is written");
            assert_eq!(
                format!("{read:?}"),
                format!(
                    "{:?}",
                    translated_as(json, "json", None, None, repeated, whole)
                ),
                "whole: {whole}"
            );
            assert_eq!(read_through_a_reader(&fifo), !whole, "whole: {whole}");
            let _ = std::fs::remove_file(&fifo);
        }
    }

    /// Every element of every array, and every member of every map, in
    /// `value`.
    fn elements(value: &tabnas::Value) -> usize {
        match value {
            tabnas::Value::Array(items) => items.len() + items.iter().map(elements).sum::<usize>(),
            tabnas::Value::ListRef(list) => {
                list.value.len() + list.value.iter().map(elements).sum::<usize>()
            }
            tabnas::Value::Object(map) => map.len() + map.values().map(elements).sum::<usize>(),
            tabnas::Value::MapRef(map) => {
                map.value.len() + map.value.values().map(elements).sum::<usize>()
            }
            _ => 0,
        }
    }

    /// What a run of `text`'s rule passes leaves of the engine's tree,
    /// through [`Format::pruned`] or not.
    fn left(format: &Format, text: &str, prune: bool) -> usize {
        let source = format.passes(text).expect("the format is parsed");
        let source = if prune { format.pruned(source) } else { source };
        let (outcome, _, value) = source.run_owned_with_value(Vec::new());
        outcome.expect("the passes stream");
        elements(&value.expect("a parse that completed returns a value"))
    }

    /// A grammar transduce lists as prunable has the engine's arrays
    /// emptied behind the stream: two thousand records leave the tree
    /// holding what two hundred do, where a run that does not prune holds
    /// every one. An unlisted grammar is left as it is, and runs.
    #[test]
    fn a_listed_grammars_tree_is_emptied_behind_the_stream() {
        let csv = |n: usize| {
            let mut text = String::from("a,b\n");
            for i in 0..n {
                text.push_str(&format!("{i},x{i}\n"));
            }
            text
        };
        let jsonl = |n: usize| {
            let mut text = String::new();
            for i in 0..n {
                text.push_str(&format!("{{\"a\":{i}}}\n"));
            }
            text
        };
        let json = |n: usize| {
            let items: Vec<String> = (0..n).map(|i| format!("{{\"a\":{i}}}")).collect();
            format!("[{}]", items.join(","))
        };
        let docs: [(&str, &dyn Fn(usize) -> String); 3] =
            [("csv", &csv), ("jsonl", &jsonl), ("json", &json)];
        for (id, doc) in docs {
            let listed = format(id).expect("a format");
            assert!(capability::prunable(id), "{id}");
            assert_eq!(
                left(listed, &doc(200), true),
                left(listed, &doc(2000), true),
                "{id}: pruned"
            );
            assert!(
                left(listed, &doc(2000), false) >= left(listed, &doc(200), false) + 1800,
                "{id}: not pruned"
            );
        }
        let toml = format("toml").expect("toml is a format");
        assert!(!capability::prunable("toml"));
        let text = "a = 1\n[b]\nc = [1, 2, 3]\n";
        assert_eq!(left(toml, text, true), left(toml, text, false));
        assert!(left(toml, text, true) > 0);
    }

    /// Every format with a parser answers its rule passes; one read as a
    /// tree has none and refuses; json5's check runs first.
    #[test]
    fn a_format_with_a_parser_gives_its_rule_passes() {
        let parsed: Vec<&str> = formats()
            .iter()
            .filter(|f| f.parses())
            .map(Format::id)
            .collect();
        assert_eq!(
            parsed,
            [
                "css", "csv", "feed", "ini", "json", "json5", "jsonc", "jsonic", "jsonl",
                "markdown", "pgn", "semver", "toml", "xml", "yaml", "zon"
            ]
        );
        assert_eq!(
            parser_names(),
            "css, csv, feed, ini, json, json5, jsonc, jsonic, jsonl, markdown, pgn, semver, toml, \
             xml, yaml or zon"
        );
        let abnf = format("abnf").expect("abnf is a format");
        assert!(!abnf.parses());
        let refused = abnf
            .passes("x = \"y\"")
            .err()
            .expect("a tree has no passes");
        assert_eq!(refused.code, Code::InputInvalid);
        assert!(
            refused.message.starts_with("abnf is read as a tree"),
            "{refused}"
        );
        let json5 = format("json5").expect("json5 is a format");
        let empty = json5
            .passes("// only a comment")
            .err()
            .expect("the check refuses");
        assert_eq!(empty.code, Code::InputInvalid);
        let (outcome, passes) = json5
            .passes("{a: 1}")
            .expect("the check passes")
            .run_owned(Vec::new());
        outcome.expect("the passes stream");
        assert!(!passes.is_empty());
    }
}
