//! 设计文档 06 § 10.7 错误码表 ↔ CLI `ErrorCode` 一致性守卫。
//!
//! **本守卫要防的失败模式**（M4 核实，见 MDR-021 § 连带修正）：06 § 10.7 的错误码表
//! 声称这些是「CLI 失败时输出的 `error_code`」，且同节要求编排器按 `VALIDATION_TIMEOUT` /
//! `VALIDATION_OOM` / `VALIDATION_SCHEMA_CORRUPTED` 三者之一分流「工具故障 vs 产出物失效」。
//! 实测这三个码在 CLI 源码中**零命中**——编排器照做则判据恒为假，一切工具故障都被误判
//! 成产出物失效而进入无意义重试。这与 #86 修的 `--status` 值域漂移是同一类失败模式：
//! 权威文档给出编排器会照抄的具体值，而该值实现里不存在。
//!
//! **判据设计**：表已按 M4 订正加了「当前实际返回」列，把语义码与实际返回值分开登记。
//! 故守卫不要求「表中每个码名都存在」（多数是 Plugin 提示词层的语义标签，本就未落地），
//! 而是要求：
//!
//! 1. 表里以 `` `E0NN` `` 形态给出的**实际返回码**必须在 `ErrorCode` 真值域内（防写错码号）。
//! 2. `ErrorCode` 的**每个**码都必须在 06 § 10.7 内被提及（防新增码不登记）。
//! 3. 三个已证实不存在的 `VALIDATION_*` 码不得再以「CLI 会返回」的形态出现（防漂回）。
//!
//! 真值域取自 `ErrorCode::iter()`，不写死清单，故新增变体会让断言 2 立即报红。
//!
//! **Markdown 渲染语义**：沿用 `design_command_table.rs` 的教训（#86 异构交叉审查实证：
//! 整张表包进 `<!-- -->` 后读者看不到任何表格而逐行扫描仍读到「影子表」，9 测试全绿）
//! ——代码块与 HTML 注释内的内容一律不算「读者看到的声明」。

use std::collections::BTreeSet;
use std::path::PathBuf;

use rustmigrate_core::error::ErrorCode;
use strum::IntoEnumIterator;

/// 设计文档 06 的路径。
fn design_06_path() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/cli -> crates -> cli -> repo root
    let repo_root = manifest.ancestors().nth(3).unwrap();
    repo_root.join("docs/design/06-plugin-structure.md")
}

/// 读取 06 § 10.7 节的可见正文（排除代码块与 HTML 注释）。
///
/// 只取该节是为了让判据有明确边界：`E0NN` 码号在别处（如 MDR 引用、沿革说明）出现
/// 不构成「CLI 返回值声明」。节的起止按 Markdown 标题识别。
fn visible_section_10_7() -> String {
    let text = std::fs::read_to_string(design_06_path()).expect("读取 06 设计文档");
    let mut out = String::new();
    let mut in_section = false;
    let mut in_fenced_code = false;
    let mut in_html_comment = false;

    for line in text.lines() {
        let trimmed = line.trim();

        // 代码块围栏：块内整段跳过（含其中的 JSON 样例——那里的 `E010` 是示例值，
        // 不是「表里声明的返回码」，重复计入会让判据边界模糊）。
        if trimmed.starts_with("```") {
            in_fenced_code = !in_fenced_code;
            continue;
        }
        if in_fenced_code {
            continue;
        }

        // HTML 注释：可同行开闭，也可跨行。注释内的一切都不渲染。
        if in_html_comment {
            if trimmed.contains("-->") {
                in_html_comment = false;
            }
            continue;
        }
        if trimmed.contains("<!--") {
            if !trimmed.contains("-->") {
                in_html_comment = true;
            }
            continue;
        }

        // 节边界：`## 10.7 …` 开始，下一个同级 `## ` 结束。
        if trimmed.starts_with("## ") {
            if in_section {
                break;
            }
            in_section = trimmed.contains("10.7");
            continue;
        }
        if in_section {
            out.push_str(line);
            out.push('\n');
        }
    }

    assert!(
        !out.trim().is_empty(),
        "未能在 06 中定位 § 10.7 节可见正文——标题格式可能已变，守卫失去作用"
    );
    out
}

/// 从文本中抽取所有 `` `E0NN` `` 形态的码号（反引号包裹，避免命中散文里的裸字母数字）。
fn extract_e_codes(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for segment in text.split('`').skip(1).step_by(2) {
        let s = segment.trim();
        // 形如 E001..E999：首字母 E + 全数字尾部。
        if let Some(digits) = s.strip_prefix('E') {
            if digits.len() >= 3 && digits.chars().all(|c| c.is_ascii_digit()) {
                out.insert(s.to_owned());
            }
        }
    }
    out
}

/// `ErrorCode` 真值域的全部码号。
fn actual_codes() -> BTreeSet<String> {
    ErrorCode::iter().map(|c| c.code().to_owned()).collect()
}

#[test]
fn design_06_declared_error_codes_all_exist_in_cli() {
    // 断言 1：06 § 10.7 里以 `E0NN` 形态给出的「当前实际返回」码必须真实存在。
    // 写错码号（如把 E008 写成 E080）会让编排器的分支永不命中，与三个 VALIDATION_*
    // 幽灵码是同一后果。
    let declared = extract_e_codes(&visible_section_10_7());
    let actual = actual_codes();

    assert!(
        !declared.is_empty(),
        "§ 10.7 未出现任何 `E0NN` 码——「当前实际返回」列可能被整体删除，守卫失去作用"
    );

    let ghosts: Vec<&String> = declared.difference(&actual).collect();
    assert!(
        ghosts.is_empty(),
        "06 § 10.7 声称 CLI 返回这些码，但 ErrorCode 里不存在（编排器按此分流会恒为假）: {ghosts:?}\n\
         CLI 实际码域: {actual:?}"
    );
}

#[test]
fn all_cli_error_codes_are_documented_in_design_06() {
    // 断言 2：反向——每个真实码都须在 § 10.7 被提及。新增 ErrorCode 变体却不登记，
    // 编排器就无从得知该如何处置它（本表是其唯一权威）。
    let section = visible_section_10_7();
    let missing: Vec<String> = ErrorCode::iter()
        .map(|c| c.code().to_owned())
        .filter(|code| !section.contains(code.as_str()))
        .collect();

    assert!(
        missing.is_empty(),
        "以下 CLI 错误码未在 06 § 10.7 登记（新增码须同步该节的「当前实际返回」列）: {missing:?}"
    );
}

#[test]
fn retired_validation_codes_are_not_reintroduced_as_cli_returns() {
    // 断言 3：三个 VALIDATION_* 码经 M4 实测确认在 CLI 中从不存在（`ValidationConfig`
    // 是空结构、无超时机制、无 schema 文件可损坏）。06 § 10.7 现已改为「按能否解析出
    // 合法 error JSON + data.retryable 判定」。此断言防漂回——一旦有人重新把它们写成
    // 「CLI 返回的 error_code」，编排器又会拿到恒假判据。
    //
    // 判据按**条目形态**而非全文出现（沿用 #86 的 `--status` 值域守卫做法）：这三个码名
    // 在「已废弃 / 不存在」的说明语境中必须能继续出现，否则订正说明本身会触发守卫。
    let section = visible_section_10_7();
    for code in [
        "VALIDATION_TIMEOUT",
        "VALIDATION_OOM",
        "VALIDATION_SCHEMA_CORRUPTED",
    ] {
        for line in section.lines() {
            if !line.contains(code) {
                continue;
            }
            // 表格条目形态：以 `|` 起头的行且首列是该码。删除线包裹（`~~`）表示已废弃，
            // 是允许的登记形态。
            let is_table_row = line.trim_start().starts_with('|');
            let marked_retired = line.contains("~~") || line.contains("不存在");
            assert!(
                !is_table_row || marked_retired,
                "{code} 在 § 10.7 以未标注废弃的表行形态出现——它在 CLI 中不存在，\n\
                 编排器按此分流会恒为假（见 MDR-021）。若确要恢复，请先在 CLI 实现该码。\n\
                 该行: {line}"
            );
        }
    }
}

#[test]
fn retryable_codes_match_design_06_table() {
    // 06 § 10.7 的「CLI 实际错误码全表」把 `data.retryable` 逐码列出，而 § 10.7 的工具故障
    // 判据要求编排器**按该字段**决定是否重试。若 `is_retryable()` 的集合变了而表未同步，
    // 编排器会对不可重试的错误反复重试（或反之放弃可恢复的瞬态 IO 故障）。
    //
    // 判据取自实现（`ErrorCode::is_retryable`），断言表里该码所在行的 retryable 列与之一致。
    let section = visible_section_10_7();

    for code in ErrorCode::iter() {
        let num = code.code();
        // 定位「CLI 实际错误码全表」中该码的行：以 `| `E0NN` |` 起头。
        let row = section
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("| `{num}` |")))
            .unwrap_or_else(|| {
                panic!("06 § 10.7 的实际码全表缺少 {num} 行（表格式变化会让本守卫失效）")
            });

        // 取 retryable 列的值。**按列位取，不按整行 contains("true")**——后者是内容
        // 启发式，只要哪一行的「含义」列出现 true 字样（例如将来某码的说明里写
        // 「…返回 true 时…」）判定就静默反转。#86 主审实证过同一教训：任何依赖行内容
        // 的判据都会被格式变体绕过。当下各行恰无无关 true，但那是巧合而非保证。
        let cols: Vec<&str> = row.trim().trim_matches('|').split('|').collect();
        assert_eq!(
            cols.len(),
            4,
            "{num} 行不是 4 列（`code | 变体 | 含义 | retryable`），表结构变化会让列位判定失效\n该行: {row}"
        );
        let retryable_cell = cols[3].trim();
        let documented_retryable = match retryable_cell.trim_matches('*').trim() {
            "true" => true,
            "false" => false,
            other => panic!(
                "{num} 的 retryable 列值非法（须为 true/false，可加 ** 强调）: {other:?}\n该行: {row}"
            ),
        };
        assert_eq!(
            documented_retryable,
            code.is_retryable(),
            "{num} 的 retryable 与设计文档不一致：实现 = {}，06 表行 = {}\n该行: {row}",
            code.is_retryable(),
            documented_retryable
        );
    }
}

#[test]
fn error_code_domain_has_expected_size() {
    // 冻结码数：新增/删除变体时强制回看上面两个断言的登记要求，避免「加了码但
    // 断言 2 恰好因散文里出现过该数字而通过」这类假绿。
    let count = ErrorCode::iter().count();
    assert_eq!(
        count, 15,
        "ErrorCode 变体数变化（{count} ≠ 15）：请同步 06 § 10.7 的「当前实际返回」列后更新本断言"
    );
}
