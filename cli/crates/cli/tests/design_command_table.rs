//! 命令清单 ↔ CLI 叶子命令一致性守卫（两处权威声明各一套）。
//!
//! CLAUDE.md 定 `docs/design/06-plugin-structure.md` 为 CLI 命令列表的唯一权威，
//! 但该表的命令行与计数一直靠人工维护——PR #85 就是修这类漂移（4 条已实现命令缺表行、
//! 表头计数失实）。本测试把「表 ↔ CLI 双向一致」变成 CI 硬门：
//!
//! - CLI 有而表无 → 新增命令漏登记（权威文档失真）。
//! - 表有而 CLI 无 → 幽灵命令（编排器照抄会 cli_parse 失败）。
//! - 表头声明的计数与表行数不符 → 计数漂移。
//!
//! **比对粒度 = 叶子命令**（`state approve` 而非 `state`）：表里登记的正是叶子。
//! 中间层 `state` / `graph` 不做实际工作——裸调只打印 help（实测退出码 0），
//! 故不该出现在命令表里。
//!
//! 文件后半段是 **`plugin/skills/migrate/SKILL.md` 命令清单**的同类守卫（见该段区块注释）。
//! 两处都是编排器会照抄的权威声明，`06:105` 表头本就要求二者同步，此前只有 06 一边有门。

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
/// `help` 排除是**防御性**的：主审实证 `Cli::command()` 此时尚未 `build()`，clap 还没注入
/// 自动生成的 `help` 子命令，故当前删掉这个分支 8 个测试依然全绿。保留它是因为
/// clap 何时注入 `help` 属实现细节（升级或改用 `command().build()` 都可能变），
/// 而 `help` 一旦混进来就是一条永远登记不进 06 表的幽灵命令。
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
    /// 落在「原 M2 推迟命令」段的命令——该段是历史沿革快照，不该增长。
    deferred_commands: BTreeSet<String>,
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
    let mut deferred_commands = BTreeSet::new();
    // 当前所处的表头段：0=未进入 1=已实现 2=原 M2 推迟
    let mut section = 0u8;
    let mut in_overview = false;
    // Markdown 语义状态。异构交叉审查（codex）实证的假绿路径：整张表包进 `<!-- -->`
    // 后读者渲染时看不到任何表格，而逐行扫描仍读得到「影子表」，9 测试全绿——守卫的
    // 全部目的正是「读者看到的表 == CLI」，故必须按渲染语义排除不可见内容。
    let mut in_html_comment = false;
    let mut in_fenced_code = false;
    // 声明出现次数：同一声明重复出现时旧写法被后值覆盖（可在注释里藏正确值、可见处写错值）。
    let mut implemented_decls = 0usize;
    let mut deferred_decls = 0usize;

    for line in text.lines() {
        let trimmed = line.trim();

        // 代码块围栏：块内整段跳过（含其中的表格样例）。
        if trimmed.starts_with("```") {
            in_fenced_code = !in_fenced_code;
            continue;
        }
        if in_fenced_code {
            continue;
        }

        // HTML 注释：可同行开闭，也可跨行。注释内的一切都不渲染，一律跳过。
        if in_html_comment {
            if trimmed.contains("-->") {
                in_html_comment = false;
            }
            continue;
        }
        if trimmed.contains("<!--") && !trimmed.contains("-->") {
            in_html_comment = true;
            continue;
        }
        if trimmed.contains("<!--") {
            // 同行开闭：注释掉的片段不参与解析。
            continue;
        }

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
            implemented_decls += 1;
            section = 1;
            continue;
        }
        if let Some(n) = parse_declared_count(line, "**原 M2 推迟命令 — ") {
            deferred.0 = n;
            deferred_decls += 1;
            section = 2;
            continue;
        }

        // 引用块 / 缩进块里的表行不是权威表的一部分——渲染成引用或代码，读者不会
        // 当作命令表读。codex 实证：把可见表改成引用表并混入新成员，段归属守卫仍 PASS。
        if trimmed.starts_with('>') || line.starts_with("    ") || line.starts_with('\t') {
            continue;
        }

        if let Some(cmd) = parse_command_from_row(line) {
            match section {
                1 => implemented.1 += 1,
                2 => {
                    deferred.1 += 1;
                    deferred_commands.insert(cmd.clone());
                }
                _ => panic!("命令行出现在任何表头声明之前，06 表结构已变: {line}"),
            }
            commands.insert(cmd);
        } else if is_table_body_row(line) {
            unparsed_rows.push(line.to_string());
        }
    }

    assert!(
        implemented_decls <= 1 && deferred_decls <= 1,
        "表头声明重复出现（已实现 {implemented_decls} 次 / 原 M2 推迟 {deferred_decls} 次）——\
         后一个声明会覆盖前一个，可被用来「可见处写错计数、别处藏正确计数」制造假绿。\
         每个声明在概览章节内应恰好出现一次"
    );

    DesignTable {
        commands,
        implemented,
        deferred,
        unparsed_rows,
        in_overview,
        deferred_commands,
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

/// 「原 M2 推迟命令」段的成员固定为当初那 5 条——它是历史沿革快照，不该增长或换人。
///
/// 审查实证的缺口：把命令从「已实现」段挪到「原 M2 推迟」段并同步两个声明数
/// （30→29、5→6），此前 3 个测试全 PASS——没有任何断言检查命令的**段归属**，
/// 而该段语义是「当初推迟、后来补上」的记录，新命令混进去会篡改设计沿革。
#[test]
fn design_06_deferred_section_membership_is_frozen() {
    let table = parse_design_table();
    let expected: BTreeSet<String> = [
        "graph rdeps",
        "graph cycles",
        "graph export",
        "validate config",
        "state update",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(
        table.deferred_commands, expected,
        "「原 M2 推迟命令」段成员应恒为当初那 5 条（历史沿革快照）——新命令请登记到\
         「已实现命令」段；若确有沿革变更，请连同本断言一起改并说明理由"
    );
}

///
/// 守卫解析器本身：确保它真的从 06 抽到了东西。
///
/// 注意它的价值**不是**防「空集比空集」假绿——主审实证订正：表侧解析为空时
/// `cli.difference(&table.commands)` 等于全部 35 条命令，前两个测试必然失败，不存在假绿。
/// 真实价值是**改善归因**：把「35 条命令全未登记」这种噪声报错，换成「解析器与表格式脱节」。
#[test]
fn design_table_parser_extracts_nonempty_set() {
    let table = parse_design_table();
    // 阈值取 CLI 叶子命令总数而非写死 30——审查实证 `>= 30` 挡不住「整个第二段丢失」
    // （第一段恰好 30 条、正好卡在阈值上）。
    let expected = cli_leaf_commands().len();
    assert!(
        table.commands.len() >= expected,
        "从 06 只解析出 {} 条命令，少于 CLI 叶子命令总数 {expected}——解析器与表格式已脱节，\
         或有整段表被漏扫",
        table.commands.len()
    );
    // 抽样锚点：覆盖不同层级**且跨两个表段**——三个锚点全在第一段时，
    // 整个第二段丢失也不会被发现（审查实证）。`graph rdeps` 属「原 M2 推迟」段。
    for anchor in [
        "init",
        "graph build",
        "state batch-transition-done",
        "graph rdeps",
    ] {
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

// ---------------------------------------------------------------------------
// Markdown 渲染语义边界（异构交叉审查 codex important 1）
//
// 守卫的命题是「**读者看到的**表 == CLI」。逐行扫描原始文本读不出渲染语义，于是
// 「渲染后不可见」的内容也被当权威表读——codex 实证把整张表包进 `<!-- -->`（读者
// 看不到任何表格）后 9 测试全绿；编排器独立复现确认。以下钉住四类不可见内容。
// ---------------------------------------------------------------------------

/// HTML 注释里的表行不算权威表——注释内容不渲染，读者看不到。
#[test]
fn html_commented_rows_are_invisible_to_the_scan() {
    // 跨行注释：整张已实现表被注释掉 → 该段行数为 0（而非照旧读到 1）。
    let doc = synthetic_doc("").replace(
        "| `rustmigrate init` | 说明 |",
        "<!--\n| `rustmigrate init` | 说明 |\n-->",
    );
    let table = scan_table(&doc);
    assert_eq!(
        table.implemented,
        (1, 0),
        "注释掉的表行不得计入（声明仍为 1，行数应为 0 → 计数守卫报错）"
    );
    assert!(
        !table.commands.contains("init"),
        "注释内的命令不该进入解析集合，否则读者看不到的「影子表」可冒充权威表"
    );

    // 同行开闭的注释同样不可见。
    let inline = synthetic_doc("").replace(
        "| `rustmigrate init` | 说明 |",
        "<!-- | `rustmigrate init` | 说明 | -->",
    );
    assert_eq!(scan_table(&inline).implemented, (1, 0), "同行注释亦不可见");
}

/// 代码块围栏内的表格是示例，不是权威表。
#[test]
fn fenced_code_block_rows_are_not_command_rows() {
    let doc = synthetic_doc("").replace(
        "| `rustmigrate init` | 说明 |",
        "```markdown\n| `rustmigrate init` | 说明 |\n```",
    );
    let table = scan_table(&doc);
    assert_eq!(table.implemented, (1, 0), "代码块内表行不得计入");
    assert!(!table.commands.contains("init"));
}

/// 引用块（`>`）与缩进块里的表行渲染成引用/代码，读者不当命令表读。
///
/// codex 实证：把可见表改成引用表并混入新成员，段归属守卫仍 PASS。
#[test]
fn blockquoted_and_indented_rows_are_not_command_rows() {
    let quoted = synthetic_doc("").replace(
        "| `rustmigrate graph export` | 说明 | 理由 |",
        "> | `rustmigrate stats community` | 混入的新成员 | 理由 |",
    );
    let table = scan_table(&quoted);
    assert_eq!(table.deferred, (1, 0), "引用块内表行不得计入");
    assert!(
        !table.deferred_commands.contains("stats community"),
        "引用表混入的成员不得污染段归属集合（否则冻结清单可被绕过）"
    );

    let indented = synthetic_doc("").replace(
        "| `rustmigrate init` | 说明 |",
        "    | `rustmigrate init` | 说明 |",
    );
    assert_eq!(scan_table(&indented).implemented, (1, 0), "缩进块亦不计入");
}

/// 表头声明重复出现要直接失败——否则可「可见处写错计数、注释外另处藏正确计数」。
///
/// 旧写法下后一个声明覆盖前一个，codex 实证可借此让计数守卫读到正确值而读者看到 999。
#[test]
#[should_panic(expected = "表头声明重复出现")]
fn duplicate_declared_counts_are_rejected() {
    let doc = synthetic_doc("").replace(
        "**原 M2 推迟命令 — 1 个（均已实现）**：",
        "**已实现命令 — 999 个**（伪造的第二个声明）：\n\
         \n\
         **原 M2 推迟命令 — 1 个（均已实现）**：",
    );
    scan_table(&doc);
}

// ─────────────────────────────────────────────────────────────────────────────
// SKILL.md 命令清单 ↔ CLI 叶子命令一致性守卫
//
// `06` 表那一边已由上方测试钉死，但 `06:105` 表头同时要求同步 SKILL.md 清单，而
// `SKILL.md:31` 自称「已穷举顶层子命令」——那次是一次性人工验证（PR #85），零自动化
// 检查。新增命令时这一边仍会漂，且编排器直接读 SKILL.md，漂了就照抄不存在的命令。
//
// 格式与 06 的 Markdown 表格**不同**，故不能复用 `parse_design_table`：清单是锚点行
// 之后连续的 8 个「`- **<分组>**：`cmd` 、`cmd`…`」行内反引号列表。两个坑（均实测确认，
// 非照抄记账）：
//
// ⒜ **必须先精确锚定清单区块**。按行格式（`- **X**：`）全文件匹配会命中 13 行——
//    「守护」「恢复」「幂等」「待签批」等 5 个同格式段落在文档别处，把它们拖进来会产出
//    大批伪幽灵命令。故以「命令清单 + 已穷举」锚点行定位，按缩进量收尾。
// ⒝ **必须限定只取命令项**。这 8 行里还散落着值域与状态名的反引号（`started`/`ok`/
//    `error`/`timeout`/`agent_done`/`advanced:false`/`reviewing → done`/`rule_version`
//    /`--to`/`--status` 等），无脑抽取行内所有反引号会产出十余条伪幽灵。判据取
//    「首 token 是 CLI 顶层子命令名」——它来自 `Cli::command()` 而非写死清单。
// ─────────────────────────────────────────────────────────────────────────────

/// SKILL.md 的路径。
fn skill_md_path() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest.ancestors().nth(3).unwrap();
    repo_root.join("plugin/skills/migrate/SKILL.md")
}

/// CLI 顶层子命令名（`state` / `graph` / `init` …），用于判定一个反引号项是否是命令。
fn cli_top_level_names() -> BTreeSet<String> {
    Cli::command()
        .get_subcommands()
        .map(|s| s.get_name().to_owned())
        .filter(|n| n != "help")
        .collect()
}

/// 从命令清单里抽出的一个分组。
#[derive(Debug, PartialEq, Eq)]
struct SkillGroup {
    label: String,
    commands: Vec<String>,
}

/// 剥掉参数占位符，只留子命令路径。
///
/// 逐 token 在首个占位符处停止：`<m>` 位置参数、`[--flag]` 可选项、`--flag` 裸选项。
/// 记账曾预警「`graph export [--format json|dot|mermaid]` 的 `json|dot|mermaid]` 会残留
/// 成命令名的一部分」——实测该风险只存在于「正则替换占位符」的实现方式，逐 token break
/// 在遇到 `[--format` 时即停止，管道段落在 break 之后、根本到不了。此处如实记录实测结论。
fn strip_placeholders(cmd: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for tok in cmd.split_whitespace() {
        if tok.starts_with('<') || tok.starts_with('[') || tok.starts_with("--") {
            break;
        }
        parts.push(tok);
    }
    parts.join(" ")
}

/// 解析 SKILL.md 的命令清单区块。
///
/// 只接受锚点行之后**缩进两格**的 `- **<分组>**：` 行；第一个不满足的行即区块结束
/// （文档紧随其后就有一个同格式但缩进为零的 `- **\`profile --adapter-tools\` 路径…**` 项，
/// 只靠格式判断会把它误收进来）。
fn parse_skill_command_list(text: &str) -> Vec<SkillGroup> {
    let top = cli_top_level_names();
    let mut groups = Vec::new();
    let mut in_list = false;

    for line in text.lines() {
        if !in_list {
            // 锚点：命令清单那一行（同时含「命令清单」与「已穷举」，避免匹配到别处提及）。
            if line.contains("命令清单") && line.contains("已穷举") {
                in_list = true;
            }
            continue;
        }
        // 区块内的分组项必须缩进两格；缩进为零的下一个一级列表项即结束。
        let Some(rest) = line.strip_prefix("  - **") else {
            break;
        };
        let Some((label, body)) = rest.split_once("**：") else {
            continue;
        };
        let commands = body
            .split('`')
            // 反引号成对包裹，奇数下标才是被包裹内容。
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, seg)| seg)
            // 只保留「首 token 是 CLI 顶层子命令名」的项，滤掉值域/状态名/裸选项。
            .filter(|seg| {
                seg.split_whitespace()
                    .next()
                    .is_some_and(|first| top.contains(first))
            })
            .map(strip_placeholders)
            .filter(|c| !c.is_empty())
            .collect();
        groups.push(SkillGroup {
            label: label.to_owned(),
            commands,
        });
    }
    groups
}

/// SKILL.md 清单里登记的全部命令。
fn skill_listed_commands() -> BTreeSet<String> {
    let text = std::fs::read_to_string(skill_md_path()).expect("读 SKILL.md 失败");
    parse_skill_command_list(&text)
        .into_iter()
        .flat_map(|g| g.commands)
        .collect()
}

#[test]
fn skill_md_command_list_matches_cli() {
    let cli = cli_leaf_commands();
    let listed = skill_listed_commands();

    let missing: Vec<_> = cli.difference(&listed).cloned().collect();
    assert!(
        missing.is_empty(),
        "SKILL.md 命令清单缺少这些已实现命令（它自称「已穷举顶层子命令」，编排器据此选命令）: {missing:?}"
    );

    let ghost: Vec<_> = listed.difference(&cli).cloned().collect();
    assert!(
        ghost.is_empty(),
        "SKILL.md 列出了 CLI 中不存在的命令（编排器照抄会 cli_parse 失败）: {ghost:?}"
    );
}

/// 分组结构冻结：防某个分组整行被删后「缺失」检查仍因其它分组齐全而假绿。
///
/// 与 06 侧的「表头计数」守卫同理——那里是计数漂移，这里是整行消失。
///
/// **它与 `skill_md_command_list_matches_cli` 不可互相替代**（变异实证）：摘掉区块收尾
/// 判定（即坑⒜，让扫描漏进文档别处 5 个同格式段落）时，本测试报红并列出多出来的
/// `L1 存在性` / `L2 结构校验` / `幂等`，而 `matches_cli` 却 **PASS**——那些段落恰好不含
/// 合法命令名、被命令项判据滤空了。故只写 `matches_cli` 会让区块锚定零覆盖。
#[test]
fn skill_md_command_list_groups_are_frozen() {
    let text = std::fs::read_to_string(skill_md_path()).expect("读 SKILL.md 失败");
    let groups = parse_skill_command_list(&text);

    let labels: Vec<&str> = groups.iter().map(|g| g.label.as_str()).collect();
    assert_eq!(
        labels,
        vec![
            "建图/查图",
            "状态推进",
            "签批门（MDR-019）",
            "度量/台账",
            "断点续跑（ROB-01a/b/c）",
            "校验",
            "统计/度量",
            "其他",
        ],
        "命令清单分组结构变了——新增/删除分组须同步本断言（防整行丢失后静默）"
    );

    // 每个分组都必须真的解析出命令，否则「命令项判据」与文案格式已脱节。
    for g in &groups {
        assert!(
            !g.commands.is_empty(),
            "分组 `{}` 未解析出任何命令——判据与格式脱节，或该行命令被清空",
            g.label
        );
    }

    // 阈值取实际 CLI 命令数，不写死——新增命令时不必改这里。
    let total: usize = groups.iter().map(|g| g.commands.len()).sum();
    assert_eq!(
        total,
        cli_leaf_commands().len(),
        "清单命令总数应与 CLI 叶子命令数相等"
    );
}

/// 解析器边界：喂合成字符串，覆盖两个坑的判据。
///
/// 沿用本文件既有惯例（06 侧解析器亦如此测）——不改真实 SKILL.md，故多个审查视角并发时
/// 无人需抢文件，且边界可回归。
#[test]
fn skill_list_parser_only_takes_command_items_inside_the_list_block() {
    let doc = "\
# 前言

- 命令清单（**已穷举顶层子命令**；参数非穷举）：
  - **甲组**：`graph build --root [--full]`、`graph export [--format json|dot|mermaid]`
  - **乙组**：`state record-subagent-call --step-index`（`--status` 只接受 `started`/`ok`/`error`/`timeout`）、`state resume`
- **`profile --adapter-tools` 路径自动解析**：`init` 这里提到的不算清单项

### 别处章节

  - **守护**：`done`/`blocked`/`graduate` 拒绝
  - **幂等**：`data.was_noop=true`
";
    let groups = parse_skill_command_list(doc);

    // 坑⒜：区块在第一个非「缩进两格」项处收尾，别处同格式段落不进来。
    let labels: Vec<&str> = groups.iter().map(|g| g.label.as_str()).collect();
    assert_eq!(labels, vec!["甲组", "乙组"], "区块外的同格式段落不得混入");

    // 坑⒝：值域/状态名/裸选项反引号不得被当成命令；管道占位符不得残留在命令名里。
    assert_eq!(
        groups[0].commands,
        vec!["graph build", "graph export"],
        "占位符须剥净（含 `[--format json|dot|mermaid]` 这种带管道的）"
    );
    assert_eq!(
        groups[1].commands,
        vec!["state record-subagent-call", "state resume"],
        "`--status` / `started` / `ok` / `error` / `timeout` 是值域，不是命令"
    );
}

/// 反向：判据不因「命令名恰好是某状态名的前缀」等情况误滤合法命令。
///
/// `graduate` 既是命令名、也在别处作为状态词出现在反引号里——清单内它必须被收进来。
#[test]
fn skill_list_parser_keeps_commands_that_double_as_state_words() {
    let doc = "\
- 命令清单（**已穷举顶层子命令**）：
  - **其他**：`init`、`graduate`（项目级毕业评估）
";
    let groups = parse_skill_command_list(doc);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].commands, vec!["init", "graduate"]);
}
