// Fleet conformance for the package-local structural translation interface.
// The macro reads every package's fields directly at its call site: that is
// the compile-time contract without introducing a shared crate type. Moved
// here from tabnas-alchemy's tests, since it renders each grammar's sample
// on transduce's routers and render's renderers; the composition that only
// compiles stays in alchemy's translation_parts_test.rs. The transducer
// part a package may ship (its manifest's `translate.transduce`) is held
// here too: present when declared and absent when not, under the entry the
// package names, read by the command, and composing over the format's
// parser's rule passes.

mod common;

use std::sync::{Arc, Mutex};

use serde_json::Value;
use tabnas_alchemy::shared::{replay, Fail, Limits, Metrics, OwnedJsonEvent};
use tabnas_alchemy::translate::{compose_parsed, Options};
use tabnas_alchemy::{Program, Source};
use tabnas_dmog::translate as cli_translate;
use tabnas_transduce::ParserSource;

use common::compile_sources;

#[derive(Clone, Copy)]
struct StructuralPart {
    entry: &'static str,
    source: Option<&'static str>,
}

struct StructuralParts {
    manifest: &'static str,
    lift: Option<StructuralPart>,
    render: Option<StructuralPart>,
    transduce: Option<StructuralPart>,
}

/// A package's parts, read field by field; a package whose interface
/// carries a transducer says so with `transduce`.
macro_rules! structural_parts {
    ($parts:expr) => {{
        let parts = $parts.expect("the grammar exposes translation parts");
        StructuralParts {
            manifest: parts.manifest,
            lift: parts.lift.map(|part| StructuralPart {
                entry: part.entry,
                source: part.source,
            }),
            render: parts.render.map(|part| StructuralPart {
                entry: part.entry,
                source: part.source,
            }),
            transduce: None,
        }
    }};
    ($parts:expr, transduce) => {{
        let parts = $parts.expect("the grammar exposes translation parts");
        StructuralParts {
            manifest: parts.manifest,
            lift: parts.lift.map(|part| StructuralPart {
                entry: part.entry,
                source: part.source,
            }),
            render: parts.render.map(|part| StructuralPart {
                entry: part.entry,
                source: part.source,
            }),
            transduce: parts.transduce.map(|part| StructuralPart {
                entry: part.entry,
                source: part.source,
            }),
        }
    }};
}

fn assert_part(format: &str, kind: &str, declared: Option<&str>, part: Option<StructuralPart>) {
    let Some(declared) = declared else {
        assert!(part.is_none(), "{format} exposes an undeclared {kind}");
        return;
    };
    let part = part.unwrap_or_else(|| panic!("{format} does not expose its declared {kind}"));
    assert!(
        !part.entry.is_empty(),
        "{format} {kind} has no explicit entry"
    );
    if declared.ends_with(".alc") {
        assert!(
            part.source.is_some_and(|source| !source.is_empty()),
            "{format} {kind} does not embed {declared}"
        );
    } else {
        assert_eq!(
            part.entry, declared,
            "{format} {kind} does not name its builtin"
        );
        assert!(
            part.source.is_none(),
            "{format} builtin {kind} unexpectedly has source"
        );
    }
}

fn conformance_main(parts: &StructuralParts, reads: &str, writes: &str, lifted: bool) -> String {
    let render = parts.render.expect("the descriptor declares a render");
    // The producer is structural fact: only the preferred shape may come
    // from a lift; every other shape is the grammar's raw tree. The adapter
    // is selected from the descriptor, so a false reads value type-fails.
    let source = if lifted {
        format!(
            "{} (events input)",
            parts.lift.expect("a lifted read shape has a lift").entry
        )
    } else {
        "events input".to_string()
    };
    let mut definitions = String::new();
    let adapted = match (reads, writes) {
        (reads, writes) if reads == writes => source,
        ("tree", "records") => {
            definitions.push_str(concat!(
                "def conformance-binding\n  record\n",
                "    entry :columns :infer\n",
                "    entry :rows (path each-index)\n\n",
            ));
            format!("table-from-json conformance-binding ({source})")
        }
        ("records", "tree") => format!("records ({source})"),
        (_, "text") => {
            definitions.push_str(concat!(
                "def conformance-text [input]\n",
                "  join \"\"\n",
                "    map\n",
                "      fn [event]\n",
                "        match event\n",
                "          case (scalar value) (scalar-text csv-options value)\n",
                "          case _ \"\"\n",
                "      input\n\n",
            ));
            format!("conformance-text ({source})")
        }
        _ => panic!("no {reads}-to-{writes} conformance adapter"),
    };
    let call = if render.entry == "csv" {
        format!("csv csv-options ({adapted})")
    } else {
        format!("{} ({adapted})", render.entry)
    };
    format!("{definitions}def export [input]\n  {call}\n")
}

fn grammar(format: &str) -> tabnas::Tabnas {
    match format {
        "css" => tabnas_css::make(),
        "csv" => tabnas_csv::make(),
        "ini" => tabnas_ini::make(),
        "json" => tabnas_json::make(),
        "json5" => tabnas_json5::make(),
        "jsonc" => tabnas_jsonc::make(),
        "jsonic" => tabnas_jsonic::make(),
        "jsonl" => tabnas_jsonl::make(),
        "markdown" => tabnas_markdown::make(),
        "toml" => tabnas_toml::make(),
        "xml" => tabnas_xml::make(),
        "yaml" => tabnas_yaml::make(),
        "zon" => tabnas_zon::make(),
        _ => panic!("no grammar for {format}"),
    }
}

fn events(format: &str, text: &str) -> Result<Vec<OwnedJsonEvent>, Fail> {
    let (outcome, events) = ParserSource::new(grammar(format), text).run_owned(Vec::new());
    outcome.map(|_| events)
}

#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Shared {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("output lock").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn render(program: &Program, events: &[OwnedJsonEvent]) -> Result<String, Fail> {
    let buffer = Shared::default();
    let mut sink = program.sink(
        Box::new(buffer.clone()),
        None,
        &Limits::default(),
        Metrics::new(),
    )?;
    replay(events, &mut sink)?;
    let bytes = buffer.0.lock().expect("output lock").clone();
    Ok(String::from_utf8(bytes).expect("the render is UTF-8"))
}

#[test]
fn structural_translation_parts_agree_with_their_descriptors_and_compile() {
    let samples = [
        (
            "css",
            "a, b { color: red } /* c */ @media screen { p { m: 1 } }",
        ),
        ("csv", "a,b\n1,x\n2,y\n"),
        ("ini", "a=1\nb=x\n"),
        ("json", "{\"a\":1,\"b\":[true,null]}\n"),
        ("json5", "{a:1,b:['x',true]}\n"),
        ("jsonc", "{\"a\":1,/* c */\"b\":true}\n"),
        ("jsonic", "a:1,b:true"),
        ("jsonl", "{\"a\":1}\n{\"a\":2}\n"),
        ("markdown", "| a | b |\n| - | - |\n| 1 | x |\n"),
        ("toml", "a = 1\nb = \"x\"\n"),
        ("xml", "<a x=\"1\">text</a>"),
        ("yaml", "a: 1\nb:\n  - x\n  - y\n"),
        ("zon", ".{ .a = 1, .b = .{ true, false } }"),
    ];
    let formats = [
        ("css", structural_parts!(tabnas_css::translate(), transduce)),
        ("csv", structural_parts!(tabnas_csv::translate(), transduce)),
        ("ini", structural_parts!(tabnas_ini::translate(), transduce)),
        (
            "json",
            structural_parts!(tabnas_json::translate(), transduce),
        ),
        (
            "json5",
            structural_parts!(tabnas_json5::translate(), transduce),
        ),
        (
            "jsonc",
            structural_parts!(tabnas_jsonc::translate(), transduce),
        ),
        (
            "jsonic",
            structural_parts!(tabnas_jsonic::translate(), transduce),
        ),
        (
            "jsonl",
            structural_parts!(tabnas_jsonl::translate(), transduce),
        ),
        (
            "markdown",
            structural_parts!(tabnas_markdown::translate(), transduce),
        ),
        (
            "toml",
            structural_parts!(tabnas_toml::translate(), transduce),
        ),
        ("xml", structural_parts!(tabnas_xml::translate(), transduce)),
        (
            "yaml",
            structural_parts!(tabnas_yaml::translate(), transduce),
        ),
        ("zon", structural_parts!(tabnas_zon::translate(), transduce)),
    ];

    for (format, parts) in formats {
        let descriptor: Value =
            serde_json::from_str(parts.manifest).expect("the embedded descriptor is JSON");
        assert_eq!(descriptor["languageId"], format, "{format} languageId");
        let translate = &descriptor["translate"];
        let reads: Vec<&str> = match &translate["reads"] {
            Value::String(shape) => vec![shape],
            Value::Array(shapes) => shapes
                .iter()
                .map(|shape| shape.as_str().expect("each read shape is a string"))
                .collect(),
            value => panic!("{format} reads is {value}"),
        };
        assert!(!reads.is_empty(), "{format} declares no read shape");
        assert!(
            reads
                .iter()
                .all(|shape| matches!(*shape, "text" | "records" | "tree")),
            "{format} reads {reads:?}"
        );
        let writes = translate["writes"].as_str().expect("writes is a string");
        assert!(
            matches!(writes, "text" | "records" | "tree"),
            "{format} writes {writes}"
        );

        let lift_path = translate["lift"].as_str();
        let render_path = translate["render"].as_str();
        assert_part(format, "lift", lift_path, parts.lift);
        assert_part(format, "render", render_path, parts.render);

        // The transducer part, where the manifest names one: the package
        // exposes it under its entry, the command reads it from the
        // package (`Format::transducer`), and it composes over the
        // format's parser's rule passes into a tree target. Where the
        // manifest names none, neither has one.
        let transduce_path = translate["transduce"].as_str();
        assert_part(format, "transduce", transduce_path, parts.transduce);
        let registered = cli_translate::format(format)
            .unwrap_or_else(|| panic!("{format} is one of the command's formats"));
        assert_eq!(
            registered.transducer().map(|alc| alc.entry.as_str()),
            parts.transduce.map(|part| part.entry),
            "{format}: the command's transducer is the package's"
        );
        if let (Some(alc), Some(path)) = (registered.transducer(), transduce_path) {
            assert!(
                alc.file.ends_with(path),
                "{format}: the transducer is {}, not {path}",
                alc.file
            );
            let target = &cli_translate::format("json")
                .expect("json is a format")
                .part;
            let composition =
                compose_parsed(&registered.part, alc, target, &Options::default(), "parts")
                    .unwrap_or_else(|fail| {
                        panic!("{format}: the transducer does not compose: {fail}")
                    });
            composition
                .compile(None, common::routers(), common::renderers())
                .unwrap_or_else(|fail| {
                    panic!("{format}: the transducer's composition does not compile: {fail}")
                });
        }

        let mut preferred = None;
        for (index, read_shape) in reads.iter().enumerate() {
            let main = conformance_main(
                &parts,
                read_shape,
                writes,
                index == 0 && parts.lift.is_some(),
            );
            let file = format!("{format}/{read_shape}-conformance.alc");
            let mut sources = vec![Source::new(&file, &main)];
            for (path, part) in [(lift_path, parts.lift), (render_path, parts.render)] {
                if let (
                    Some(path),
                    Some(StructuralPart {
                        source: Some(source),
                        ..
                    }),
                ) = (path, part)
                {
                    sources.push(Source::new(path, source));
                }
            }
            let program = compile_sources(&sources).unwrap_or_else(|fail| {
                panic!("{format} reads {read_shape}: translation sources do not compile: {fail}")
            });
            for (kind, part) in [("lift", parts.lift), ("render", parts.render)] {
                if let Some(StructuralPart {
                    entry,
                    source: Some(_),
                    ..
                }) = part
                {
                    assert!(
                        program.resolved().get(entry).is_some(),
                        "{format} {kind} source does not define {entry}"
                    );
                }
            }
            if index == 0 {
                preferred = Some(program);
            }
        }
        let program = preferred.expect("each format has a preferred read shape");

        let sample = samples
            .iter()
            .find_map(|(name, text)| (*name == format).then_some(*text))
            .expect("each format has a sample");
        let first = events(format, sample)
            .unwrap_or_else(|fail| panic!("{format} does not parse its sample: {fail}"));
        let rendered = render(&program, &first)
            .unwrap_or_else(|fail| panic!("{format} translation parts do not render: {fail}"));
        let second = events(format, &rendered).unwrap_or_else(|fail| {
            panic!("{format} does not parse its own render {rendered:?}: {fail}")
        });
        let rerendered = render(&program, &second)
            .unwrap_or_else(|fail| panic!("{format} second render fails: {fail}"));
        assert_eq!(
            rerendered, rendered,
            "{format} render is not round-trip stable"
        );
    }
}
