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
}

fn parse_design_table() -> DesignTable {
    let path = design_06_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("读 {} 失败: {e}", path.display()));

    let mut commands = BTreeSet::new();
    let mut implemented = (0usize, 0usize);
    let mut deferred = (0usize, 0usize);
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
        }
    }

    assert!(
        in_overview || implemented.0 > 0,
        "未在 06 中找到「### CLI 命令概览」章节，表结构已变"
    );
    DesignTable {
        commands,
        implemented,
        deferred,
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
