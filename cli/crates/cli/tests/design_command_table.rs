//! 设计文档 06 命令表 ↔ CLI 叶子命令一致性守卫。
//!
//! CLAUDE.md 定 `docs/design/06-plugin-structure.md` 为 CLI 命令列表的唯一权威，
//! 但该表的命令行与计数一直靠人工维护——PR #85 就是修这类漂移（4 条已实现命令缺表行、
//! 表头计数失实）。本测试把「表 ↔ CLI 双向一致」变成 CI 硬门：
//!
//! - CLI 有而表无 → 新增命令漏登记（权威文档失真）。
//! - 表有而 CLI 无 → 幽灵命令（编排器照抄会 cli_parse 失败）。
//! - 表头声明的计数与表行数不符 → 计数漂移。
//!
//! **比对粒度 = 叶子命令**（`state approve` 而非 `state`）：表里登记的正是叶子，
//! 中间层 `state` / `graph` 自身不可单独执行。

use std::collections::BTreeSet;
use std::path::PathBuf;

use clap::CommandFactory;
use rustmigrate_cli::Cli;

/// 设计文档 06 的路径。
fn design_06_path() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/cli -> crates -> cli -> repo root
    let repo_root = manifest.ancestors().nth(3).unwrap();
    repo_root.join("docs/design/06-plugin-structure.md")
}

/// 递归收集 clap 命令树的全部叶子命令路径（不含 `rustmigrate` 前缀）。
///
/// clap 自动生成的 `help` 子命令不是业务命令，排除。
fn collect_leaf_commands(cmd: &clap::Command, prefix: &str, out: &mut BTreeSet<String>) {
    let mut has_child = false;
    for sub in cmd.get_subcommands() {
        let name = sub.get_name();
        if name == "help" {
            continue;
        }
        has_child = true;
        let path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix} {name}")
        };
        collect_leaf_commands(sub, &path, out);
    }
    if !has_child && !prefix.is_empty() {
        out.insert(prefix.to_string());
    }
}

/// CLI 实际暴露的全部叶子命令。
fn cli_leaf_commands() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    collect_leaf_commands(&Cli::command(), "", &mut out);
    out
}

/// 从 06 的 Markdown 表格行提取命令名。
///
/// 表格首列形如 `` | `rustmigrate state deps <module>` | 说明 | ``——取反引号内文本、
/// 剥 `rustmigrate ` 前缀，并丢掉 `<module>` / `[--flag]` 之类的参数占位符（只留子命令路径）。
fn parse_command_from_row(line: &str) -> Option<String> {
    let first_cell = line.strip_prefix('|')?.split('|').next()?.trim();
    let inner = first_cell
        .strip_prefix('`')?
        .split('`')
        .next()?
        .trim()
        .strip_prefix("rustmigrate ")?;
    let path: Vec<&str> = inner
        .split_whitespace()
        .take_while(|tok| !tok.starts_with('<') && !tok.starts_with('[') && !tok.starts_with('-'))
        .collect();
    if path.is_empty() {
        None
    } else {
        Some(path.join(" "))
    }
}

/// 这一行是否落在表格体内、按构造**必须**是一条命令行。
///
/// 判据按**位置**而非内容：概览章节内、以 `|` 开头、既不是分隔行（`|---|`）也不是
/// 列标题行（首列为 `子命令`）的行，就是表格体的数据行——06 的这两张表除命令外不放别的行。
///
/// 早先版本按内容判（首列 `contains("rustmigrate")`），被审查实证击穿：把
/// `` | `rustmigrate state deps <module>` | `` 写成 `` | `state deps <module>` | ``（丢命令名前缀）
/// 时该行既解析不出、又不满足启发式，于是被静默当作非命令行跳过，最终仍误报成
/// 「CLI 命令未登记」。任何依赖行内容的判据都会被格式变体绕过，故改为按位置定性。
fn is_table_body_row(line: &str) -> bool {
    let Some(rest) = line.strip_prefix('|') else {
        return false;
    };
    let Some(first_cell) = rest.split('|').next() else {
        return false;
    };
    let cell = first_cell.trim();
    // 分隔行 `|---|---|`；列标题行 `| 子命令 | 说明 |`。
    !cell.starts_with('-') && !cell.starts_with(':') && cell != "子命令"
}

/// 06 表登记的命令集合 + 两段表头声明的计数。
///
/// 只扫「### CLI 命令概览」到下一个 `### ` 之间的区间——避免把 § 10 命令清单、
/// SubAgent 表等其它位置的命令提及误当作表行。
struct DesignTable {
    commands: BTreeSet<String>,
    /// 「已实现命令 — N 个」段：(声明数, 实际行数)
    implemented: (usize, usize),
    /// 「原 M2 推迟命令 — N 个（均已实现）」段：(声明数, 实际行数)
    deferred: (usize, usize),
    /// 看着像命令行、但解析器没认出来的行（格式与解析器脱节的信号）。
    unparsed_rows: Vec<String>,
    /// 是否找到并进入过「### CLI 命令概览」章节。
    in_overview: bool,
}

fn parse_design_table() -> DesignTable {
    let path = design_06_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("读 {} 失败: {e}", path.display()));
    let table = scan_table(&text);

    assert!(
        table.in_overview || table.implemented.0 > 0,
        "未在 06 中找到「### CLI 命令概览」章节，表结构已变"
    );
    // 扫描完整性后置条件：两段表头声明都必须命中。
    //
    // 扫描在概览章节后的下一个 `### ` 处 break——若有人在概览章节内新增 `###` 子标题，
    // 后半段表会被静默截断，只剩「计数 vs 行数」偶然相符的假绿。没有这条断言，
    // 截断表现为「命令未登记」，归因同样误导（审查实证指出）。
    assert!(
        table.implemented.0 > 0 && table.deferred.0 > 0,
        "只扫到 {} 段表头声明（已实现={} / 原 M2 推迟={}）——概览章节内很可能新增了 `###` 子标题\
         导致扫描提前终止、后半段表被静默截断。请改用 `####` 或调整本测试的章节边界判定。",
        u8::from(table.implemented.0 > 0) + u8::from(table.deferred.0 > 0),
        table.implemented.0,
        table.deferred.0
    );
    table
}

/// 纯扫描：不做断言，供合成字符串单元测试直接调用。
fn scan_table(text: &str) -> DesignTable {
    let mut commands = BTreeSet::new();
    let mut implemented = (0usize, 0usize);
    let mut deferred = (0usize, 0usize);
    let mut unparsed_rows = Vec::new();
    // 当前所处的表头段：0=未进入 1=已实现 2=原 M2 推迟
    let mut section = 0u8;
    let mut in_overview = false;

    for line in text.lines() {
        if line.starts_with("### ") {
            // 概览章节结束即停止扫描。
            if in_overview {
                break;
            }
            in_overview = line.contains("CLI 命令概览");
            continue;
        }
        if !in_overview {
            continue;
        }

        if let Some(n) = parse_declared_count(line, "**已实现命令 — ") {
            implemented.0 = n;
            section = 1;
            continue;
        }
        if let Some(n) = parse_declared_count(line, "**原 M2 推迟命令 — ") {
            deferred.0 = n;
            section = 2;
            continue;
        }

        if let Some(cmd) = parse_command_from_row(line) {
            commands.insert(cmd);
            match section {
                1 => implemented.1 += 1,
                2 => deferred.1 += 1,
                _ => panic!("命令行出现在任何表头声明之前，06 表结构已变: {line}"),
            }
        } else if is_table_body_row(line) {
            unparsed_rows.push(line.to_string());
        }
    }

    DesignTable {
        commands,
        implemented,
        deferred,
        unparsed_rows,
        in_overview,
    }
}

/// 从 `**已实现命令 — 30 个**（…` 这类表头提取声明的数字。
fn parse_declared_count(line: &str, marker: &str) -> Option<usize> {
    let rest = line.trim().strip_prefix(marker)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

#[test]
fn design_06_table_matches_cli_leaf_commands() {
    let cli = cli_leaf_commands();
    let table = parse_design_table();

    // 先报归因：解析不了的表行会连带表现为「命令未登记」，若不先点明，维护者会
    // 去表里找那条并不缺失的行。
    assert!(
        table.unparsed_rows.is_empty(),
        "以下 06 表行看着是命令行但解析器没认出来（表格式与本测试的解析器脱节，\
         先修格式或解析器，否则下面的「未登记」判定不可信）: {:?}",
        table.unparsed_rows
    );

    let missing: Vec<_> = cli.difference(&table.commands).collect();
    let ghost: Vec<_> = table.commands.difference(&cli).collect();

    assert!(
        missing.is_empty(),
        "以下 CLI 命令未登记到 docs/design/06-plugin-structure.md 的命令表（该表是唯一权威）: {missing:?}"
    );
    assert!(
        ghost.is_empty(),
        "06 命令表登记了 CLI 中不存在的命令（编排器照抄会 cli_parse 失败）: {ghost:?}"
    );
}

#[test]
fn design_06_table_declared_counts_match_row_counts() {
    let table = parse_design_table();

    assert_eq!(
        table.implemented.0, table.implemented.1,
        "06「已实现命令 — {} 个」表头计数与实际 {} 行不符",
        table.implemented.0, table.implemented.1
    );
    assert_eq!(
        table.deferred.0, table.deferred.1,
        "06「原 M2 推迟命令 — {} 个」表头计数与实际 {} 行不符",
        table.deferred.0, table.deferred.1
    );
    assert_eq!(
        table.implemented.1 + table.deferred.1,
        cli_leaf_commands().len(),
        "两段表行数之和应等于 CLI 叶子命令总数"
    );
}

/// 守卫解析器本身：若 06 表格式变化导致抽取不到命令，上面两个测试会「空集比空集」假绿。
#[test]
fn design_table_parser_extracts_nonempty_set() {
    let table = parse_design_table();
    assert!(
        table.commands.len() >= 30,
        "从 06 只解析出 {} 条命令，解析器与表格式已脱节（预期 ≥30）",
        table.commands.len()
    );
    // 抽样锚点：三段不同层级的命令都应被解析到。
    for anchor in ["init", "graph build", "state batch-transition-done"] {
        assert!(
            table.commands.contains(anchor),
            "解析结果缺锚点命令 `{anchor}`，解析器可能只吃到部分表行"
        );
    }
}

// ---------------------------------------------------------------------------
// 解析器单元测试：喂合成字符串，不改 06 文档
//
// 早先验证这些边界靠「临时改 06 → 跑测试 → git checkout 还原」，在多审查视角
// 并发跑的工作区里会互相冲掉改动（本 PR 审查期间真实发生）。改为合成字符串后，
// 边界可回归、无人需要抢文件；新发现的格式变体只需往下面加一条用例。
// ---------------------------------------------------------------------------

/// 正常表行能解析出子命令路径，且参数占位符被剥掉。
#[test]
fn parse_row_extracts_subcommand_path_without_placeholders() {
    let cases = [
        ("| `rustmigrate init` | 说明 |", "init"),
        ("| `rustmigrate graph build` | 说明 |", "graph build"),
        // `<module>` 位置参数剥掉。
        ("| `rustmigrate state deps <module>` | 说明 |", "state deps"),
        // `[--flag]` / `--flag` 剥掉。
        (
            "| `rustmigrate state reset --module <M> [--force]` | 说明 |",
            "state reset",
        ),
        (
            "| `rustmigrate state batch-transition-done --module <M>...` | 说明 |",
            "state batch-transition-done",
        ),
    ];
    for (line, expected) in cases {
        assert_eq!(
            parse_command_from_row(line).as_deref(),
            Some(expected),
            "行 {line:?} 应解析为 {expected:?}"
        );
    }
}

/// 表格结构行（分隔行 / 列标题行）不得被当作命令行或未解析行。
#[test]
fn table_structure_rows_are_not_command_rows() {
    for line in ["|--------|------|", "| :--- | ---: |", "| 子命令 | 说明 |"] {
        assert!(
            !is_table_body_row(line),
            "{line:?} 是表格结构行，不应计入表体数据行"
        );
        assert!(
            parse_command_from_row(line).is_none(),
            "{line:?} 不应解析出命令"
        );
    }
    // 非表格行同样不算。
    for line in ["普通段落", "> 引用", "**已实现命令 — 30 个**：", ""] {
        assert!(!is_table_body_row(line), "{line:?} 不是表格行");
    }
}

/// 格式变体：解析不出命令时，必须被认定为「表体行」进而计入 unparsed_rows。
///
/// 这是归因正确性的核心——每一条都曾（或可能）被静默跳过、最终误报成
/// 「CLI 命令未登记」，让维护者去表里找并不缺失的行。
#[test]
fn malformed_command_rows_are_flagged_not_silently_skipped() {
    let variants = [
        // 双反引号（本 PR 自查发现）。
        "| ``rustmigrate init`` | 说明 |",
        // 丢 `rustmigrate ` 前缀（审查实证发现——旧的内容启发式在此被击穿）。
        "| `state deps <module>` | 说明 |",
        // 完全没有反引号。
        "| rustmigrate init | 说明 |",
        // 加粗包裹。
        "| **`rustmigrate init`** | 说明 |",
        // 前导反引号被替换成加粗标记（审查实证期间真实出现的形态：`**rustmigrate init` ）。
        "| **rustmigrate init` | 说明 |",
        // 只有命令名前缀、无子命令。
        "| `rustmigrate` | 说明 |",
        // 首列只有占位符。
        "| `rustmigrate <cmd>` | 说明 |",
    ];
    for line in variants {
        assert!(
            parse_command_from_row(line).is_none(),
            "用例前提失效：{line:?} 竟能被解析出命令，请改用例或更新解析器"
        );
        assert!(
            is_table_body_row(line),
            "{line:?} 解析不出命令却未被判为表体行——会被静默跳过并误报成「命令未登记」"
        );
    }
}

/// 合成一份最小 06 骨架：两段表头 + 各一条命令行。
fn synthetic_doc(overview_extra: &str) -> String {
    format!(
        "### CLI 命令概览\n\
         \n\
         **已实现命令 — 1 个**（…）：\n\
         \n\
         | 子命令 | 说明 |\n\
         |--------|------|\n\
         | `rustmigrate init` | 说明 |\n\
         {overview_extra}\
         **原 M2 推迟命令 — 1 个（均已实现）**：\n\
         \n\
         | 子命令 | 说明 | 理由 |\n\
         |--------|------|------|\n\
         | `rustmigrate graph export` | 说明 | 理由 |\n\
         \n\
         ### 下一章节\n\
         \n\
         | `rustmigrate ghost-outside-overview` | 不该被扫到 |\n"
    )
}

/// 正常骨架：两段都扫到，且概览章节外的表行不被计入。
#[test]
fn scan_covers_both_sections_and_stops_at_next_heading() {
    let table = scan_table(&synthetic_doc(""));
    assert!(table.in_overview);
    assert_eq!(table.implemented, (1, 1), "已实现段：声明数与行数");
    assert_eq!(table.deferred, (1, 1), "原 M2 推迟段：声明数与行数");
    assert!(table.unparsed_rows.is_empty(), "{:?}", table.unparsed_rows);
    assert_eq!(
        table
            .commands
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["graph export", "init"],
        "概览章节后的 `### 下一章节` 内表行不得被计入"
    );
}

/// 概览章节内新增 `###` 子标题 → 扫描提前终止、后半段静默截断。
///
/// 这是审查指出的隐患：截断后「计数 vs 行数」在前半段仍自洽，只有完整性后置条件
/// 能把它抓出来。这里先证明截断确实发生（`deferred` 全 0），再证明 `parse_design_table`
/// 的后置断言会因此报错。
#[test]
fn scan_truncates_when_overview_gains_a_h3_subheading() {
    let truncated = scan_table(&synthetic_doc("\n### 插入的子标题\n\n"));
    assert_eq!(
        truncated.implemented,
        (1, 1),
        "前半段仍自洽——所以单看计数无法发现截断"
    );
    assert_eq!(
        truncated.deferred,
        (0, 0),
        "后半段被静默截断（这正是完整性后置条件要抓的情形）"
    );

    // 后置断言的实际文案与触发条件（parse_design_table 走文件，这里直接复核条件）。
    assert!(
        !(truncated.implemented.0 > 0 && truncated.deferred.0 > 0),
        "完整性后置条件应判定为不满足，从而报「后半段表被静默截断」而非「命令未登记」"
    );
}
