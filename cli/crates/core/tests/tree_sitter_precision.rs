use rustmigrate_core::lang::typescript::TypeScriptAdapter;
use rustmigrate_core::lang::{FileAnalysis, LanguageAdapter};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// 将 FileAnalysis 转换为旧格式的字符串集合，用于精度对比。
fn to_string_sets(analysis: &FileAnalysis) -> (HashSet<String>, HashSet<String>, HashSet<String>) {
    let mut imports = HashSet::new();
    let mut calls = HashSet::new();

    let exports: HashSet<String> = analysis.exported_names.clone();

    for imp in &analysis.imports {
        if imp.is_side_effect {
            imports.insert(format!("<-{}", imp.module_path));
        } else if imp.is_dynamic {
            imports.insert(format!("dynamic<-{}", imp.module_path));
        } else {
            for sym in &imp.symbols {
                let prefix = if imp.is_type_only { "type:" } else { "" };
                if sym.is_namespace {
                    let alias = sym.alias.as_deref().unwrap_or("*");
                    imports.insert(format!("{prefix}*:{alias}<-{}", imp.module_path));
                } else if sym.is_default {
                    imports.insert(format!("{prefix}default:{}<-{}", sym.name, imp.module_path));
                } else {
                    imports.insert(format!("{prefix}{}<-{}", sym.name, imp.module_path));
                }
            }
        }
    }

    for call in &analysis.calls {
        if call.is_constructor {
            calls.insert(format!("new:{}", call.callee));
        } else {
            calls.insert(format!("call:{}", call.callee));
        }
    }

    (exports, imports, calls)
}

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

    let mut adapter = TypeScriptAdapter::new();
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
        let analysis = adapter.analyze_file(&source, truth.file).unwrap();
        let (pred_exports, pred_imports, pred_calls) = to_string_sets(&analysis);

        let expected_exports: HashSet<String> =
            truth.exports.iter().map(|s| s.to_string()).collect();
        let expected_imports: HashSet<String> =
            truth.imports.iter().map(|s| s.to_string()).collect();
        let expected_calls: HashSet<String> = truth.calls.iter().map(|s| s.to_string()).collect();

        let (ep, er, ef) = f1_score(&pred_exports, &expected_exports);
        let (ip, ir, i_f) = f1_score(&pred_imports, &expected_imports);
        let (cp, cr, cf) = f1_score(&pred_calls, &expected_calls);

        total_export_tp += pred_exports.intersection(&expected_exports).count();
        total_export_pred += pred_exports.len();
        total_export_truth += expected_exports.len();

        total_import_tp += pred_imports.intersection(&expected_imports).count();
        total_import_pred += pred_imports.len();
        total_import_truth += expected_imports.len();

        total_call_tp += pred_calls.intersection(&expected_calls).count();
        total_call_pred += pred_calls.len();
        total_call_truth += expected_calls.len();

        if ef < 1.0 || i_f < 1.0 || cf < 1.0 {
            let mut detail = format!("--- {} ---\n", truth.file);
            if ef < 1.0 {
                let missing: Vec<_> = expected_exports.difference(&pred_exports).collect();
                let extra: Vec<_> = pred_exports.difference(&expected_exports).collect();
                detail += &format!(
                    "  EXPORTS P={ep:.2} R={er:.2} F1={ef:.2}\n    missing={missing:?}\n    extra={extra:?}\n"
                );
            }
            if i_f < 1.0 {
                let missing: Vec<_> = expected_imports.difference(&pred_imports).collect();
                let extra: Vec<_> = pred_imports.difference(&expected_imports).collect();
                detail += &format!(
                    "  IMPORTS P={ip:.2} R={ir:.2} F1={i_f:.2}\n    missing={missing:?}\n    extra={extra:?}\n"
                );
            }
            if cf < 1.0 {
                let missing: Vec<_> = expected_calls.difference(&pred_calls).collect();
                let extra: Vec<_> = pred_calls.difference(&expected_calls).collect();
                detail += &format!(
                    "  CALLS   P={cp:.2} R={cr:.2} F1={cf:.2}\n    missing={missing:?}\n    extra={extra:?}\n"
                );
            }
            failures.push(detail);
        }
    }

    let macro_export_f1 = macro_f1(total_export_tp, total_export_pred, total_export_truth);
    let macro_import_f1 = macro_f1(total_import_tp, total_import_pred, total_import_truth);
    let macro_call_f1 = macro_f1(total_call_tp, total_call_pred, total_call_truth);

    println!("\n========== tree-sitter TS 精度报告 (via TypeScriptAdapter) ==========");
    println!("EXPORTS  F1={macro_export_f1:.3}");
    println!("IMPORTS  F1={macro_import_f1:.3}");
    println!("CALLS    F1={macro_call_f1:.3}");

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
}

fn macro_f1(tp: usize, pred: usize, truth: usize) -> f64 {
    let p = if pred > 0 {
        tp as f64 / pred as f64
    } else {
        1.0
    };
    let r = if truth > 0 {
        tp as f64 / truth as f64
    } else {
        1.0
    };
    if p + r > 0.0 {
        2.0 * p * r / (p + r)
    } else {
        0.0
    }
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

    let mut adapter = TypeScriptAdapter::new();

    let p = root.join("fixtures/linear-deps/src/index.ts");
    let source = fs::read_to_string(&p).unwrap();
    let analysis = adapter.analyze_file(&source, "index.ts").unwrap();
    let (exports, imports, calls) = to_string_sets(&analysis);
    assert!(
        imports.iter().any(|i| i.contains("./service")),
        "should import from service: {imports:?}"
    );
    assert!(exports.contains("clamp") || exports.contains("Range"));

    let p = root.join("fixtures/linear-deps/src/utils.ts");
    let source = fs::read_to_string(&p).unwrap();
    let analysis = adapter.analyze_file(&source, "utils.ts").unwrap();
    let (exports, _, _) = to_string_sets(&analysis);
    assert!(exports.contains("clamp"));
    assert!(exports.contains("fetchData"));
    assert!(exports.contains("Range"));

    let p = root.join("fixtures/edge-cases/src/empty.ts");
    let source = fs::read_to_string(&p).unwrap();
    let analysis = adapter.analyze_file(&source, "empty.ts").unwrap();
    let (exports, imports, calls) = to_string_sets(&analysis);
    assert!(exports.is_empty() && imports.is_empty() && calls.is_empty());

    println!("✓ 所有现有 fixture 文件验证通过 (via TypeScriptAdapter)");
}
