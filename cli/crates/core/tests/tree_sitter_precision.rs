use rustmigrate_core::ts_extract::TsExtractor;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

struct SnippetTruth {
    file: &'static str,
    exports: &'static [&'static str],
    imports: &'static [&'static str],
    calls: &'static [&'static str],
}

fn ground_truth() -> Vec<SnippetTruth> {
    vec![
        SnippetTruth {
            file: "01_named_export_func.ts",
            exports: &["add", "multiply"],
            imports: &[],
            calls: &[],
        },
        SnippetTruth {
            file: "02_named_export_class.ts",
            exports: &["Logger", "Formatter"],
            imports: &[],
            calls: &["call:console.log", "call:s.trim"],
        },
        SnippetTruth {
            file: "03_named_export_const.ts",
            exports: &["PI", "counter", "legacy"],
            imports: &[],
            calls: &[],
        },
        SnippetTruth {
            file: "04_named_export_types.ts",
            exports: &["Config", "Handler", "LogLevel"],
            imports: &[],
            calls: &[],
        },
        SnippetTruth {
            file: "05_default_export_func.ts",
            exports: &["default", "main"],
            imports: &[],
            calls: &["call:console.log"],
        },
        SnippetTruth {
            file: "06_default_export_class.ts",
            exports: &["default", "App"],
            imports: &[],
            calls: &["call:console.log"],
        },
        SnippetTruth {
            file: "07_default_export_expr.ts",
            exports: &["default"],
            imports: &[],
            calls: &[],
        },
        SnippetTruth {
            file: "08_reexport_named.ts",
            exports: &["foo", "baz", "defaultItem"],
            imports: &[],
            calls: &[],
        },
        SnippetTruth {
            file: "09_reexport_star.ts",
            exports: &["*<-./utils", "*<-./types"],
            imports: &[],
            calls: &[],
        },
        SnippetTruth {
            file: "10_reexport_star_as.ts",
            exports: &["utils", "types"],
            imports: &[],
            calls: &[],
        },
        SnippetTruth {
            file: "11_import_named.ts",
            exports: &[],
            imports: &[
                "readFile<-fs",
                "writeFile<-fs",
                "join<-path",
                "resolve<-path",
            ],
            calls: &["call:readFile", "call:join"],
        },
        SnippetTruth {
            file: "12_import_default.ts",
            exports: &[],
            imports: &["default:express<-express", "default:cors<-cors"],
            calls: &["call:express", "call:cors", "call:app.use"],
        },
        SnippetTruth {
            file: "13_import_star.ts",
            exports: &[],
            imports: &["*:path<-path", "*:fs<-fs"],
            calls: &["call:path.join", "call:fs.readFileSync"],
        },
        SnippetTruth {
            file: "14_import_mixed.ts",
            exports: &["Counter"],
            imports: &[
                "default:React<-react",
                "useState<-react",
                "useEffect<-react",
            ],
            calls: &["call:useState", "call:useEffect", "call:console.log"],
        },
        SnippetTruth {
            file: "15_import_type_only.ts",
            exports: &["greet"],
            imports: &[
                "type:User<-./types",
                "type:Session<-./types",
                "type:Config<-./config",
            ],
            calls: &[],
        },
        SnippetTruth {
            file: "16_export_type_only.ts",
            exports: &["Config", "Handler", "User"],
            imports: &[],
            calls: &[],
        },
        SnippetTruth {
            file: "17_dynamic_import.ts",
            exports: &[],
            imports: &["dynamic<-./plugins/core", "dynamic<-./themes/dark"],
            calls: &[],
        },
        SnippetTruth {
            file: "18_side_effect_import.ts",
            exports: &[],
            imports: &["<-./polyfill", "<-./setup"],
            calls: &["call:console.log"],
        },
        SnippetTruth {
            file: "19_calls_simple.ts",
            exports: &[],
            imports: &[
                "process<-./utils",
                "validate<-./utils",
                "transform<-./utils",
            ],
            calls: &["call:validate", "call:process", "call:transform"],
        },
        SnippetTruth {
            file: "20_calls_method_ctor.ts",
            exports: &[],
            imports: &["EventEmitter<-events", "Logger<-./logger"],
            calls: &[
                "new:EventEmitter",
                "new:Logger",
                "call:emitter.on",
                "call:emitter.emit",
                "call:logger.info",
                "call:console.log",
            ],
        },
    ]
}

fn f1_score(predicted: &HashSet<String>, truth: &HashSet<String>) -> (f64, f64, f64) {
    if truth.is_empty() && predicted.is_empty() {
        return (1.0, 1.0, 1.0);
    }
    if truth.is_empty() || predicted.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let tp = predicted.intersection(truth).count() as f64;
    let precision = tp / predicted.len() as f64;
    let recall = tp / truth.len() as f64;
    let f1 = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };
    (precision, recall, f1)
}

#[test]
fn tree_sitter_precision_benchmark() {
    let bench_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures/ts-precision-bench/snippets");

    let mut ext = TsExtractor::new();
    let truths = ground_truth();

    let mut total_export_tp = 0usize;
    let mut total_export_pred = 0usize;
    let mut total_export_truth = 0usize;

    let mut total_import_tp = 0usize;
    let mut total_import_pred = 0usize;
    let mut total_import_truth = 0usize;

    let mut total_call_tp = 0usize;
    let mut total_call_pred = 0usize;
    let mut total_call_truth = 0usize;

    let mut failures = Vec::new();

    for truth in &truths {
        let path = bench_dir.join(truth.file);
        let source = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", truth.file));
        let result = ext.extract(&source, &path).unwrap();

        let expected_exports: HashSet<String> =
            truth.exports.iter().map(|s| s.to_string()).collect();
        let expected_imports: HashSet<String> =
            truth.imports.iter().map(|s| s.to_string()).collect();
        let expected_calls: HashSet<String> = truth.calls.iter().map(|s| s.to_string()).collect();

        let (ep, er, ef) = f1_score(&result.exports, &expected_exports);
        let (ip, ir, i_f) = f1_score(&result.imports, &expected_imports);
        let (cp, cr, cf) = f1_score(&result.calls, &expected_calls);

        total_export_tp += result.exports.intersection(&expected_exports).count();
        total_export_pred += result.exports.len();
        total_export_truth += expected_exports.len();

        total_import_tp += result.imports.intersection(&expected_imports).count();
        total_import_pred += result.imports.len();
        total_import_truth += expected_imports.len();

        total_call_tp += result.calls.intersection(&expected_calls).count();
        total_call_pred += result.calls.len();
        total_call_truth += expected_calls.len();

        if ef < 1.0 || i_f < 1.0 || cf < 1.0 {
            let mut detail = format!("--- {} ---\n", truth.file);
            if ef < 1.0 {
                let missing: Vec<_> = expected_exports.difference(&result.exports).collect();
                let extra: Vec<_> = result.exports.difference(&expected_exports).collect();
                detail += &format!(
                    "  EXPORTS P={ep:.2} R={er:.2} F1={ef:.2}\n    missing={missing:?}\n    extra={extra:?}\n"
                );
            }
            if i_f < 1.0 {
                let missing: Vec<_> = expected_imports.difference(&result.imports).collect();
                let extra: Vec<_> = result.imports.difference(&expected_imports).collect();
                detail += &format!(
                    "  IMPORTS P={ip:.2} R={ir:.2} F1={i_f:.2}\n    missing={missing:?}\n    extra={extra:?}\n"
                );
            }
            if cf < 1.0 {
                let missing: Vec<_> = expected_calls.difference(&result.calls).collect();
                let extra: Vec<_> = result.calls.difference(&expected_calls).collect();
                detail += &format!(
                    "  CALLS   P={cp:.2} R={cr:.2} F1={cf:.2}\n    missing={missing:?}\n    extra={extra:?}\n"
                );
            }
            failures.push(detail);
        }
    }

    let macro_export_p = if total_export_pred > 0 {
        total_export_tp as f64 / total_export_pred as f64
    } else {
        1.0
    };
    let macro_export_r = if total_export_truth > 0 {
        total_export_tp as f64 / total_export_truth as f64
    } else {
        1.0
    };
    let macro_export_f1 = if macro_export_p + macro_export_r > 0.0 {
        2.0 * macro_export_p * macro_export_r / (macro_export_p + macro_export_r)
    } else {
        0.0
    };

    let macro_import_p = if total_import_pred > 0 {
        total_import_tp as f64 / total_import_pred as f64
    } else {
        1.0
    };
    let macro_import_r = if total_import_truth > 0 {
        total_import_tp as f64 / total_import_truth as f64
    } else {
        1.0
    };
    let macro_import_f1 = if macro_import_p + macro_import_r > 0.0 {
        2.0 * macro_import_p * macro_import_r / (macro_import_p + macro_import_r)
    } else {
        0.0
    };

    let macro_call_p = if total_call_pred > 0 {
        total_call_tp as f64 / total_call_pred as f64
    } else {
        1.0
    };
    let macro_call_r = if total_call_truth > 0 {
        total_call_tp as f64 / total_call_truth as f64
    } else {
        1.0
    };
    let macro_call_f1 = if macro_call_p + macro_call_r > 0.0 {
        2.0 * macro_call_p * macro_call_r / (macro_call_p + macro_call_r)
    } else {
        0.0
    };

    println!("\n========== tree-sitter TS 精度报告 ==========");
    println!(
        "EXPORTS  P={:.3} R={:.3} F1={:.3}  (TP={} pred={} truth={})",
        macro_export_p,
        macro_export_r,
        macro_export_f1,
        total_export_tp,
        total_export_pred,
        total_export_truth
    );
    println!(
        "IMPORTS  P={:.3} R={:.3} F1={:.3}  (TP={} pred={} truth={})",
        macro_import_p,
        macro_import_r,
        macro_import_f1,
        total_import_tp,
        total_import_pred,
        total_import_truth
    );
    println!(
        "CALLS    P={:.3} R={:.3} F1={:.3}  (TP={} pred={} truth={})",
        macro_call_p, macro_call_r, macro_call_f1, total_call_tp, total_call_pred, total_call_truth
    );

    if !failures.is_empty() {
        println!("\n========== 不完美匹配 ==========");
        for f in &failures {
            println!("{f}");
        }
    }

    assert!(
        macro_export_f1 >= 0.90,
        "Export F1 {macro_export_f1:.3} < 0.90"
    );
    assert!(
        macro_import_f1 >= 0.90,
        "Import F1 {macro_import_f1:.3} < 0.90"
    );
    assert!(macro_call_f1 >= 0.90, "Call F1 {macro_call_f1:.3} < 0.90");

    println!("\n✓ 所有维度 F1 ≥ 0.90，tree-sitter 精度验证通过");
}

#[test]
fn tree_sitter_existing_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let mut ext = TsExtractor::new();

    let p = root.join("fixtures/linear-deps/src/index.ts");
    let linear_index = fs::read_to_string(&p).unwrap();
    let r = ext.extract(&linear_index, &p).unwrap();
    assert!(
        r.imports.contains("NumberService<-./service"),
        "linear index imports: {r:?}"
    );
    assert!(r.exports.contains("clamp"));
    assert!(r.exports.contains("Range"));
    assert!(r.exports.contains("default"));
    assert!(r.calls.contains("new:NumberService"));

    let p = root.join("fixtures/linear-deps/src/utils.ts");
    let linear_utils = fs::read_to_string(&p).unwrap();
    let r = ext.extract(&linear_utils, &p).unwrap();
    assert!(r.exports.contains("clamp"));
    assert!(r.exports.contains("fetchData"));
    assert!(r.exports.contains("Range"));
    assert!(r.exports.contains("Predicate"));

    let p = root.join("fixtures/diamond-deps/src/index.ts");
    let diamond_index = fs::read_to_string(&p).unwrap();
    let r = ext.extract(&diamond_index, &p).unwrap();
    assert!(r.imports.contains("AuthService<-./auth"));
    assert!(r.imports.contains("findUser<-./db"));
    assert!(r.imports.contains("type:User<-./types"));
    assert!(r.exports.contains("login"));
    assert!(r.exports.contains("AuthService"));

    let p = root.join("fixtures/diamond-deps/src/auth.ts");
    let diamond_auth = fs::read_to_string(&p).unwrap();
    let r = ext.extract(&diamond_auth, &p).unwrap();
    assert!(r.imports.contains("type:User<-./types"));
    assert!(r.imports.contains("findUser<-./db"));
    assert!(r.imports.contains("generateId<-./db"));
    assert!(r.exports.contains("AuthService"));
    assert!(r.calls.contains("call:findUser"));

    let p = root.join("fixtures/diamond-deps/src/barrel.ts");
    let barrel = fs::read_to_string(&p).unwrap();
    let r = ext.extract(&barrel, &p).unwrap();
    assert!(r.exports.len() >= 6, "barrel re-exports: {:?}", r.exports);

    let p = root.join("fixtures/circular-deps/src/event-bus.ts");
    let circ_bus = fs::read_to_string(&p).unwrap();
    let r = ext.extract(&circ_bus, &p).unwrap();
    assert!(r.imports.contains("Handler<-./handler"));
    assert!(r.imports.contains("type:EventName<-./shared"));
    assert!(r.exports.contains("EventBus"));

    let p = root.join("fixtures/edge-cases/src/syntax-error.ts");
    let syntax_err = fs::read_to_string(&p).unwrap();
    let r = ext.extract(&syntax_err, &p).unwrap();
    assert!(
        r.exports.contains("valid") || r.exports.contains("broken"),
        "syntax error file should still extract something: {:?}",
        r.exports
    );

    let p = root.join("fixtures/edge-cases/src/empty.ts");
    let empty = fs::read_to_string(&p).unwrap();
    let r = ext.extract(&empty, &p).unwrap();
    assert!(r.exports.is_empty() && r.imports.is_empty() && r.calls.is_empty());

    let p = root.join("fixtures/edge-cases/src/pure-types.ts");
    let types = fs::read_to_string(&p).unwrap();
    let r = ext.extract(&types, &p).unwrap();
    assert!(r.exports.contains("Config"));
    assert!(r.exports.contains("Handler"));
    assert!(r.exports.contains("LogLevel"));

    println!("✓ 所有现有 fixture 文件验证通过");
}
