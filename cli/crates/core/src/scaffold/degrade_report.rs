//! 降级分析报告。
//!
//! 3 轮编译失败后生成降级分析报告，帮用户选择降级方式（FFI / 人工 / 跳过）。

use serde::{Deserialize, Serialize};

/// 降级分析报告——编译反复失败后生成，帮用户选择降级方式。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradeReport {
    /// 失败模块名。
    pub module: String,
    /// 失败分类。
    pub failure_category: FailureCategory,
    /// 触发错误的代码片段 + 错误信息。
    pub error_snippets: Vec<ErrorSnippet>,
    /// 已尝试的修复策略。
    pub attempted_fixes: Vec<String>,
    /// 三种降级方式的预估代价。
    pub degrade_options: DegradeOptions,
}

/// 失败分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCategory {
    /// 编译错误（类型不匹配、生命周期等）。
    CompilationError,
    /// 类型复杂度过高（泛型嵌套、trait 约束过多）。
    TypeComplexity,
    /// 依赖解析失败（缺少 crate、版本冲突）。
    DependencyResolution,
    /// 语义鸿沟（源语言特性无 Rust 等价物）。
    SemanticGap,
}

/// 错误代码片段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorSnippet {
    /// 出错的文件路径。
    pub file: String,
    /// 出错的行号（0-based）。
    pub line: usize,
    /// 代码片段。
    pub code: String,
    /// 编译器错误信息。
    pub error_message: String,
}

/// 三种降级方式的预估代价。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DegradeOptions {
    /// FFI 桥接。
    pub ffi: DegradeEstimate,
    /// 人工处理。
    pub manual: DegradeEstimate,
    /// 跳过（裁剪）。
    pub skip: DegradeEstimate,
}

/// 单种降级方式的预估。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DegradeEstimate {
    /// 预估工作量：`"low"` / `"medium"` / `"high"`。
    pub effort: String,
    /// 该方式的具体说明。
    pub description: String,
    /// 对下游模块的影响。
    pub downstream_impact: String,
}

/// 从编译错误信息生成降级分析报告。
///
/// # 参数
/// - `module`: 失败模块名
/// - `error_snippets`: 触发错误的代码片段
/// - `attempted_fixes`: 已尝试的修复策略列表
/// - `export_count`: 该模块导出接口数（影响 FFI 工作量估算）
/// - `downstream_count`: 下游依赖模块数（影响跳过代价估算）
pub fn generate_degrade_report(
    module: &str,
    error_snippets: Vec<ErrorSnippet>,
    attempted_fixes: Vec<String>,
    export_count: usize,
    downstream_count: usize,
) -> DegradeReport {
    let failure_category = classify_failure(&error_snippets);

    let ffi_effort = match export_count {
        0..=3 => "low",
        4..=10 => "medium",
        _ => "high",
    };

    let degrade_options = DegradeOptions {
        ffi: DegradeEstimate {
            effort: ffi_effort.to_string(),
            description: format!(
                "生成 napi-rs FFI binding（{export_count} 个导出接口），通过 FFI 桥接调用源语言实现"
            ),
            downstream_impact: "下游模块可照常使用，性能有 FFI 调用开销".to_string(),
        },
        manual: DegradeEstimate {
            effort: "high".to_string(),
            description: "人工分析编译错误，手动修改翻译代码直至编译通过".to_string(),
            downstream_impact: "无额外影响，翻译完成后与正常模块一致".to_string(),
        },
        skip: DegradeEstimate {
            effort: "low".to_string(),
            description: "跳过该模块，从迁移范围中裁剪".to_string(),
            downstream_impact: format!(
                "{downstream_count} 个下游模块将受影响，需调整依赖或同步裁剪"
            ),
        },
    };

    DegradeReport {
        module: module.to_string(),
        failure_category,
        error_snippets,
        attempted_fixes,
        degrade_options,
    }
}

/// 根据错误信息简单分类失败原因。
fn classify_failure(snippets: &[ErrorSnippet]) -> FailureCategory {
    for snippet in snippets {
        let msg = snippet.error_message.to_lowercase();
        if msg.contains("lifetime")
            || msg.contains("borrow")
            || msg.contains("type mismatch")
            || msg.contains("expected")
            || msg.contains("mismatched types")
        {
            return FailureCategory::CompilationError;
        }
        if msg.contains("trait bound") || msg.contains("generic") || msg.contains("where clause") {
            return FailureCategory::TypeComplexity;
        }
        if msg.contains("could not find")
            || msg.contains("unresolved")
            || msg.contains("dependency")
        {
            return FailureCategory::DependencyResolution;
        }
    }
    // 默认归为语义鸿沟
    FailureCategory::SemanticGap
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_snippets() -> Vec<ErrorSnippet> {
        vec![ErrorSnippet {
            file: "src/utils.rs".to_string(),
            line: 42,
            code: "let x: &str = y.clone();".to_string(),
            error_message: "mismatched types: expected `&str`, found `String`".to_string(),
        }]
    }

    #[test]
    fn test_generate_degrade_report_basic() {
        let report = generate_degrade_report(
            "utils",
            sample_snippets(),
            vec!["尝试添加 .as_str()".to_string()],
            2,
            1,
        );

        assert_eq!(report.module, "utils");
        assert_eq!(report.failure_category, FailureCategory::CompilationError);
        assert_eq!(report.error_snippets.len(), 1);
        assert_eq!(report.attempted_fixes.len(), 1);
        assert_eq!(report.degrade_options.ffi.effort, "low");
        assert_eq!(
            report.degrade_options.skip.downstream_impact,
            "1 个下游模块将受影响，需调整依赖或同步裁剪"
        );
    }

    #[test]
    fn test_classify_failure_compilation() {
        let snippets = vec![ErrorSnippet {
            file: "a.rs".to_string(),
            line: 1,
            code: "".to_string(),
            error_message: "lifetime may not live long enough".to_string(),
        }];
        assert_eq!(
            classify_failure(&snippets),
            FailureCategory::CompilationError
        );
    }

    #[test]
    fn test_classify_failure_type_complexity() {
        let snippets = vec![ErrorSnippet {
            file: "a.rs".to_string(),
            line: 1,
            code: "".to_string(),
            error_message: "the trait bound `Foo: Bar` is not satisfied".to_string(),
        }];
        assert_eq!(classify_failure(&snippets), FailureCategory::TypeComplexity);
    }

    #[test]
    fn test_classify_failure_dependency() {
        let snippets = vec![ErrorSnippet {
            file: "a.rs".to_string(),
            line: 1,
            code: "".to_string(),
            error_message: "could not find `foo_crate` in registry".to_string(),
        }];
        assert_eq!(
            classify_failure(&snippets),
            FailureCategory::DependencyResolution
        );
    }

    #[test]
    fn test_classify_failure_semantic_gap() {
        let snippets = vec![ErrorSnippet {
            file: "a.rs".to_string(),
            line: 1,
            code: "".to_string(),
            error_message: "some obscure error".to_string(),
        }];
        assert_eq!(classify_failure(&snippets), FailureCategory::SemanticGap);
    }

    #[test]
    fn test_classify_failure_empty() {
        assert_eq!(classify_failure(&[]), FailureCategory::SemanticGap);
    }

    #[test]
    fn test_degrade_report_serialization() {
        let report = generate_degrade_report("my_module", sample_snippets(), vec![], 5, 0);

        // 序列化 → 反序列化往返
        let json = serde_json::to_string_pretty(&report).unwrap();
        let deserialized: DegradeReport = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.module, "my_module");
        assert_eq!(
            deserialized.failure_category,
            FailureCategory::CompilationError
        );
        assert_eq!(deserialized.degrade_options.ffi.effort, "medium");
        assert_eq!(
            deserialized.degrade_options.skip.downstream_impact,
            "0 个下游模块将受影响，需调整依赖或同步裁剪"
        );
    }

    #[test]
    fn test_effort_levels() {
        // export_count = 0 → ffi effort = low
        let r = generate_degrade_report("m", vec![], vec![], 0, 0);
        assert_eq!(r.degrade_options.ffi.effort, "low");

        // export_count = 5 → ffi effort = medium
        let r = generate_degrade_report("m", vec![], vec![], 5, 0);
        assert_eq!(r.degrade_options.ffi.effort, "medium");

        // export_count = 15 → ffi effort = high
        let r = generate_degrade_report("m", vec![], vec![], 15, 0);
        assert_eq!(r.degrade_options.ffi.effort, "high");

        // downstream_count = 0 → skip impact = low
        let r = generate_degrade_report("m", vec![], vec![], 0, 0);
        assert_eq!(r.degrade_options.skip.effort, "low");

        // downstream_count = 5 → skip impact = high
        let r = generate_degrade_report("m", vec![], vec![], 0, 5);
        assert_eq!(
            r.degrade_options.skip.downstream_impact,
            "5 个下游模块将受影响，需调整依赖或同步裁剪"
        );
    }
}
