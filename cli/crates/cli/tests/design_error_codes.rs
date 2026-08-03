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

/// 「CLI 实际错误码全表」的列数：`code | 变体 | 可达性 | 含义 | retryable`。
const TABLE_COLUMNS: usize = 5;

/// 表里被标注为**当前不可达**的码。主审视角逐条实证（见 MDR-021 § 可达性核查）：
/// - `E002`：`CyclicDependency` 唯一构造点在 `topological_sort`，唯一非测试调用点就地
///   match 消费后改用 `ErrorData::new` 重构造（输出无 `error_code`、退出码 2）。
/// - `E003`：`From<&MigrateError>` 中无分支映射到 `ModuleNotFound`；实测返 `E012`。
/// - `E006`：源变体 `MigrateError::Blocked` 零构造点（只有 match arm）；实测返 `E012`。
///
/// 这三个码若被当作可分流值写进编排器分支，与已废弃的 `VALIDATION_*` 幽灵码同样恒不
/// 命中——正是本守卫要防的失败模式，只是藏在「实际码全表」内部。
const UNREACHABLE_CODES: &[&str] = &["E002", "E003", "E006"];

/// 取一行表格的各列（去掉首尾 `|` 后按 `|` 切分）。
fn row_columns(row: &str) -> Vec<&str> {
    row.trim().trim_matches('|').split('|').collect()
}

/// 从文本中抽取所有 `` `E0NN` `` 形态的码号（反引号包裹，避免命中散文里的裸字母数字）。
///
/// **逐行配对**，不对整节做一次 `split('`')`：自查实测若节内任一行出现奇数个反引号
/// （中文引号误用、未闭合的行内代码等），整节此后的配对全部错位——`design_06_declared_
/// error_codes_all_exist_in_cli` 会报出无法理解的假红，把正常改文档的人拦在门外。
/// 逐行配对把错位限制在该行内，其余行仍正确解析。
fn extract_e_codes(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in text.lines() {
        // 奇数个反引号的行本身无法可靠配对，跳过（宁可漏检一行，不让错位传播全节）。
        if line.matches('`').count() % 2 != 0 {
            continue;
        }
        for segment in line.split('`').skip(1).step_by(2) {
            let s = segment.trim();
            // 形如 E001..E999：首字母 E + 全数字尾部。
            if let Some(digits) = s.strip_prefix('E') {
                if digits.len() >= 3 && digits.chars().all(|c| c.is_ascii_digit()) {
                    out.insert(s.to_owned());
                }
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
    // 断言 2：反向——每个真实码都须在「CLI 实际错误码全表」里**以表行形态**登记。
    // 新增 ErrorCode 变体却不登记，编排器就无从得知该如何处置它（本表是其唯一权威）。
    //
    // **判据是表行而非全文 contains**：自查实测 § 10.7 节内 `E008` 出现 4 次（表行 1 次 +
    // 散文 3 次，如 `SCHEMA_VERSION_UNSUPPORTED` 行的「实测返 E008」与判据段的举例），
    // `E010`/`E011`/`E012`/`E014`/`E002` 同样各有散文提及。若按全文匹配，删掉某码的表行
    // 后断言仍会因散文提及而通过——正是本守卫要消灭的那类假绿。
    let section = visible_section_10_7();
    let missing: Vec<String> = ErrorCode::iter()
        .map(|c| c.code().to_owned())
        .filter(|code| !has_table_row_for(&section, code))
        .collect();

    assert!(
        missing.is_empty(),
        "以下 CLI 错误码未在 06 § 10.7 的「CLI 实际错误码全表」中以表行形态登记: {missing:?}\n\
         （散文里提到该码号不算登记——编排器查的是表）"
    );
}

/// 该码是否在「CLI 实际错误码全表」中有自己的表行（首列为 `` `E0NN` ``）。
fn has_table_row_for(section: &str, code: &str) -> bool {
    section
        .lines()
        .any(|l| l.trim_start().starts_with(&format!("| `{code}` |")))
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
        let cols = row_columns(row);
        assert_eq!(
            cols.len(),
            TABLE_COLUMNS,
            "{num} 行不是 {TABLE_COLUMNS} 列（`code | 变体 | 可达性 | 含义 | retryable`），\
             表结构变化会让列位判定失效\n该行: {row}"
        );
        let retryable_cell = cols[TABLE_COLUMNS - 1].trim();
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
fn unreachable_codes_are_marked_as_such_in_design_06() {
    // 断言 6（主审视角提出）：表的「可达性」列必须与实际可达性一致。
    //
    // 本 PR 初版新增「CLI 实际错误码全表」时，表头写着「`data.error_code` 的完整值域，
    // 编排器分流以此为准」，却把 E002/E003/E006 三个**当前不可能出现在任何输出里**的码
    // 列成普通可分流值——等于在这张「据实纠错」的新表内部重演了它要消灭的幽灵码模式。
    //
    // **判据的真值源是 `From<&MigrateError>` 的映射覆盖**，不是两份写死清单互相比对：
    // 一个 `ErrorCode` 若在 `From` 中无任何 `MigrateError` 变体映射到它，就绝无可能出现在
    // 输出里（`ErrorData::with_error_code` 的调用方全部经由 `From` 取码）。E003 正属此类。
    // E002/E006 的不可达另有成因（构造点被就地消费 / 源变体零构造点），这两条无法从类型
    // 系统推出，故列入 `UNREACHABLE_CODES` 常量并在其 doc 中记明实证依据。
    let section = visible_section_10_7();

    for code in ErrorCode::iter() {
        let num = code.code();
        let row = section
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("| `{num}` |")))
            .unwrap_or_else(|| panic!("06 § 10.7 的实际码全表缺少 {num} 行"));

        let cols = row_columns(row);
        assert_eq!(cols.len(), TABLE_COLUMNS, "{num} 行列数异常\n该行: {row}");

        let marked_unreachable = cols[2].contains("不可达");
        let is_unreachable = UNREACHABLE_CODES.contains(&num);

        assert_eq!(
            marked_unreachable,
            is_unreachable,
            "{num} 的可达性标注与实证不符：实际{}可达，表里{}标「不可达」。\n\
             若某码的可达性因实现变化而改变，请同步 UNREACHABLE_CODES 常量与本表\n该行: {row}",
            if is_unreachable { "不" } else { "" },
            if marked_unreachable { "" } else { "未" },
        );
    }
}

#[test]
fn codes_without_error_mapping_are_all_marked_unreachable() {
    // 断言 6 的类型级补强：`From<&MigrateError>` 未覆盖的码**必然**不可达，故必须在
    // `UNREACHABLE_CODES` 里。这条不依赖人工维护的清单——它从 error.rs 的源码结构取真值，
    // 使「新增一个 ErrorCode 变体但忘了接线 From」这类漏配被立刻发现（该码会成为死码，
    // 若同时被表登记为可达就是新的幽灵码）。
    let error_rs = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .join("crates/core/src/error.rs"),
    )
    .expect("读取 error.rs");

    // 截取 `impl From<&MigrateError> for ErrorCode` 块。
    let from_block = error_rs
        .split_once("impl From<&MigrateError> for ErrorCode")
        .expect("未找到 From<&MigrateError> impl——error.rs 结构变化，本守卫需同步")
        .1;
    let from_block = from_block
        .split_once("\n}\n")
        .map(|(b, _)| b)
        .unwrap_or(from_block);

    for code in ErrorCode::iter() {
        let variant = format!("{code:?}"); // Debug 即变体名
        let mapped = from_block.contains(&format!("Self::{variant}"));
        if !mapped {
            assert!(
                UNREACHABLE_CODES.contains(&code.code()),
                "{} ({variant}) 在 `From<&MigrateError>` 中无映射分支——它绝无可能出现在输出里，\n\
                 必须列入 UNREACHABLE_CODES 并在 06 § 10.7 标「不可达」，否则编排器会按它写恒不命中的分支",
                code.code()
            );
        }
    }
}

#[test]
fn error_code_domain_has_expected_size() {
    // 冻结码数：新增/删除变体时强制回看上面两个断言的登记要求，避免「加了码但
    // 断言 2 恰好因散文里出现过该数字而通过」这类假绿。
    let count = ErrorCode::iter().count();
    assert_eq!(
        count, 15,
        "ErrorCode 变体数变化（{count} ≠ 15）。需同步的不只是本断言的数字：\n\
         ① 06 § 10.7「CLI 实际错误码全表」加一整行（`has_table_row_for` 要求表行形态，散文提及不算）；\n\
         ② 该行的「可达性」列须与 `UNREACHABLE_CODES` 一致；\n\
         ③ 若新码在 `From<&MigrateError>` 中无映射分支，它是死码，须列入 `UNREACHABLE_CODES`"
    );
}
