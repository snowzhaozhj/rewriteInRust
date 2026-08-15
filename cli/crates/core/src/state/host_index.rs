//! `member_files` 划分的宿主索引：把「一个 key 归属哪个模块」收成唯一实现。
//!
//! # 为什么要有这个模块
//!
//! composite 组折叠后 `modules` 只登记**组代表** key，组内其余成员（`member_files` 里的
//! 文件）不是独立模块。于是「一个 key 属于谁」有两个来源——登记模块自身，以及把它列为
//! 成员的组——而这两个来源**可以同时命中**，那正是 `member_files` 跨组互斥不变量被破坏
//! 的形态之一。
//!
//! 此前这一判定在仓内有**三份**各自独立的实现，且**三份都是「先查 `modules`，命中就早返回」**：
//! `validate::resolve_blocked_ref`（处置：降级告警，旧文件须可读）、
//! `MigrationStateMachine::canonical_module_key`（处置：硬错，MDR-015:55）、
//! CLI `cmd_state_deps` 的内联归一（处置：静默取 `find` 的迭代序首个）。于是「key 既是登记
//! 模块、又被别的组列为成员」这一类破坏在三条路径上**同时**被判成合法。
//!
//! 实测后果不是漏诊断而是数据损坏：`file:shared.ts` 登记为 `done` 而 `g1` 组（`translating`）
//! 把它列为成员时，`validate state` 零跨组告警、`--auto-unblock` 真的解除了引用它的模块并
//! 落盘，而该文件实际还在翻译中；`state deps` 则返回 `all_ready:true` 零告警，让依赖门禁静默
//! 放行（MDR-023）。
//!
//! 故本模块的两条设计约束：
//!
//! 1. **不早返回**。[`HostIndex::resolve`] 把两个来源的宿主**全部收集完**再判个数，
//!    互斥破坏因此无法从任何一侧溜过去。
//! 2. **唯一实现**。三处处置策略各不相同（告警 / 硬错 / 门禁拒绝）是对的，但**判定**共用
//!    这里——多份判定必然漂移，而这三份不是漂移而是**同时错在同一处**，因为它们是互相
//!    照着写的。判定失败时的用户可见说明也共用 [`broken_partition_message`]。

use std::collections::HashMap;

use crate::types::state::MigrationStateFile;

/// 一个 key 的宿主解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostResolution<'a> {
    /// 唯一宿主：它自己是登记模块，或唯一一个 composite 组把它列为成员。
    Resolved(&'a String),
    /// 宿主不唯一——`member_files` 跨组互斥不变量被破坏。宿主 key 按字典序排列。
    ///
    /// 两种形态都在此：⒜ 被 ≥2 个组列为成员；⒝ 自己是登记模块**且**被别的组列为成员。
    /// ⒝ 曾因早返回而完全检不出，见模块头。
    ///
    /// 调用方**不得**从中挑一个用：宿主组的状态各不相同，挑错就判错就绪。
    Ambiguous(Vec<&'a String>),
    /// 无处归属：既非登记模块，也不是任何组的成员。
    Missing,
}

/// 把模块 key 渲染进人读告警/错误文本。
///
/// 用 JSON 字符串字面量而非裸反引号包裹：key 来自 state 文件（可被手工编辑），
/// 若其中含反引号、`→`、`、` 或换行，裸包裹会让文本里的条目边界可被内容伪造，
/// 读者无从分辨哪部分是 key、哪部分是分隔符。JSON 转义同时处理引号与控制字符。
///
/// 原在 `validate` 模块内私有；[`broken_partition_message`] 需要同一处理（同一份数据、同一类
/// 风险给相反答案是「已有实现要沿用别重写」的反面），故上移到判定所在的本模块共用。
pub(crate) fn quote_key(key: &str) -> String {
    serde_json::to_string(key).unwrap_or_else(|_| format!("{key:?}"))
}

/// 「宿主不唯一」的统一说明文本（`member_files` 跨组互斥不变量被破坏）。
///
/// 三条路径共用它——`MigrationStateMachine::canonical_module_key` 的错误信息、
/// `batch_transition_done` 的 `skipped.detail`、CLI `state deps` 的归一失败——因为同一损坏在
/// 各条路径上各写一遍措辞必然漂移，而它是用户看到的唯一归因来源。
///
/// key 一律经 [`quote_key`] 转义，不用裸反引号。
pub fn broken_partition_message(key: &str, hosts: &[&String]) -> String {
    let quoted_key = quote_key(key);
    let hosts = hosts
        .iter()
        .map(|h| quote_key(h))
        .collect::<Vec<_>>()
        .join("、");
    format!(
        "member_files 跨组互斥不变量被破坏：{quoted_key} 的宿主不唯一（{hosts}）——归一到任何\
         一个都可能改错模块，故拒绝操作。修正 modules 的 member_files 划分后重试；若宿主清单里\
         含 {quoted_key} 自己，表示它既登记为独立模块、又被别的组列为成员，二者留一个。\
         `rustmigrate validate state` 会列出全部被破坏的划分"
    )
}

/// key → 宿主模块的一次性索引。
///
/// # 为什么是索引而不是每次全表扫
///
/// 前一版 `resolve_blocked_ref` 每查一条引用就全表扫一遍 `modules` + `member_files`。
/// 合法引用走哈希命中直接返回，故按合法数据测出来是近线性的（MDR-021 记的「10 万模块
/// 1.11s」）；但**全未命中**的坏 state 走的是全表扫那一支，实测 1000 模块 0.72s /
/// 10000 模块 4.55s / 20000 模块 14.20s（MDR-022 待办 5）。而面向坏 state 的
/// `state repair` 恰好总在这一侧。
///
/// 建表一次 O(成员总数)，之后每查 O(1)。
///
/// **本模块不提供隐藏建表的自由函数**——那正是让调用方在循环里退回二次复杂度的入口。但要如实
/// 说明一个例外：[`MigrationStateMachine::resolve_module_host`](crate::state::MigrationStateMachine::resolve_module_host)
/// 与 `canonical_module_key` 是**每次调用都重建索引**的便捷入口（单模块操作用，O(成员总数)
/// 与它们替代的旧全表扫同阶，无回归），**不得在循环里调用**：`batch_transition_done` 目前
/// 恰好是在 `for name in modules` 里调它，审查实测 200 组时每次调用 88.5ms、1000 组时 1.87s。
/// 现实 batch 规模（数十模块）下仍是亚秒级，故记账而非本轮重构（把索引提到循环外需要先在
/// 不可变借用块里把全部入参归一成 owned，才能进 `&mut self` 的 mutate 循环）。见 MDR-023
/// 后续 TODO。
pub struct HostIndex<'a> {
    state_file: &'a MigrationStateFile,
    /// 成员文件 → 把它列为成员的模块 key（**字典序、已去重**）。
    ///
    /// 两条不变量：
    ///
    /// - **不含「组代表把自己列进 `member_files`」那一项**：`populate-modules` 落盘的
    ///   `member_files` 就是全体成员、含代表自身（`DecompUnit.members` 的首个成员即组代表
    ///   key），故那不是破坏。它已由 [`resolve`](HostIndex::resolve) 的「自己是登记模块」
    ///   那一支计为一个宿主；这里再计一次，每个正常 composite 组都会被误判成跨组破坏。
    /// - **已去重**：宿主按**模块**计数、不按条目计数。同一组重复列同一文件仍是唯一宿主。
    by_member: HashMap<&'a str, Vec<&'a String>>,
}

impl<'a> HostIndex<'a> {
    /// 建索引。复杂度 O(全部 `member_files` 条目数)。
    pub fn build(state_file: &'a MigrationStateFile) -> Self {
        let mut by_member: HashMap<&'a str, Vec<&'a String>> = HashMap::new();
        for (owner, module) in &state_file.modules {
            let Some(members) = module.member_files.as_ref() else {
                continue;
            };
            for file in members {
                // 自引用不入表，理由见 `by_member` 字段注释。
                if file == owner {
                    continue;
                }
                by_member.entry(file.as_str()).or_default().push(owner);
            }
        }
        // `modules` 是 `HashMap`：不排序则多宿主告警文本的宿主顺序每次运行都漂移。
        // 排序后紧跟 `dedup`：**宿主要按模块计数，不按条目计数**。同一组的 `member_files` 里
        // 重复列同一文件是合法的（宿主仍唯一），而被删的两份旧实现用「按模块 filter」天然
        // 满足这点，改成逐条目建表后必须显式去重——否则 `Ambiguous(["grp","grp"])` 会让合法
        // 划分被判成破坏（四条按 key 归一的命令全拒 + `--auto-unblock` 永不放行）。
        for hosts in by_member.values_mut() {
            hosts.sort();
            hosts.dedup();
        }
        Self {
            state_file,
            by_member,
        }
    }

    /// 把一个 key 归一到它所属的模块。
    ///
    /// **两个来源都查完再判个数**，不在任何一侧早返回——早返回就是本模块存在的原因。
    pub fn resolve(&self, key: &str) -> HostResolution<'a> {
        let mut hosts: Vec<&'a String> = Vec::new();
        if let Some((registered, _)) = self.state_file.modules.get_key_value(key) {
            hosts.push(registered);
        }
        if let Some(from_members) = self.by_member.get(key) {
            // `by_member` 已排除自引用，故与上面 push 的那项不会重复。
            hosts.extend_from_slice(from_members);
        }
        hosts.sort();
        match hosts.len() {
            0 => HostResolution::Missing,
            1 => HostResolution::Resolved(hosts[0]),
            _ => HostResolution::Ambiguous(hosts),
        }
    }

    /// 全部被破坏的划分项：`(文件, 宿主集合)`，按文件字典序。
    ///
    /// 判据**复用** [`resolve`](HostIndex::resolve)（取其 `Ambiguous` 那一支），不另写一套
    /// 「什么算破坏」——否则告警口径与判定口径会各自漂移。
    ///
    /// 存在的意义是**不靠引用路径撞见**：跨组告警此前只在遍历 `blocked_by` 时顺带发现破坏，
    /// 于是「破坏存在但没有任何 `blocked_by` 引用到它」时 `validate state` 一声不响，而下一步
    /// 对该模块的 `transition`/`reset` 会硬错——体检说健康、动手就报错。
    pub fn broken_partitions(&self) -> Vec<(&'a str, Vec<&'a String>)> {
        // 候选只需取自 `by_member`：仅「自己是登记模块」而无任何组列它为成员时 hosts 恒为 1，
        // 不可能是 `Ambiguous`。
        let mut out: Vec<(&'a str, Vec<&'a String>)> = self
            .by_member
            .keys()
            .filter_map(|file| match self.resolve(file) {
                HostResolution::Ambiguous(hosts) => Some((*file, hosts)),
                HostResolution::Resolved(_) | HostResolution::Missing => None,
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(b.0));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::common::Timestamp;
    use crate::types::state::{
        DangerProvenance, ModuleState, ModuleStatus, ProjectState, StateHistoryEntry,
    };

    fn module(status: ModuleStatus) -> ModuleState {
        ModuleState {
            status,
            substatus: None,
            sprint: Some(1),
            attempts: Vec::new(),
            test_pass_rate: None,
            coverage: None,
            known_differences: 0,
            tier: None,
            phase_a_version: None,
            phase_a_audit_passed: None,
            blocked_by: None,
            pre_blocked_status: None,
            member_files: None,
            composite_kind: None,
            decomposition_snapshot: None,
            decomposition_frozen: false,
            danger: Vec::new(),
            danger_provenance: DangerProvenance::Unclassified,
        }
    }

    /// 只有 `modules` 对本索引有意义，其余字段取最小合法值。
    fn state_with(modules: &[(&str, ModuleState)]) -> MigrationStateFile {
        let mut state = MigrationStateFile {
            version: "1.0.0".to_owned(),
            state: ProjectState::Init,
            state_history: vec![StateHistoryEntry {
                state: ProjectState::Init,
                entered_at: Timestamp::new("2024-01-01T00:00:00Z"),
                exited_at: None,
            }],
            project: None,
            sprint: None,
            modules: HashMap::new(),
            config_ref: None,
            subagent_calls: Vec::new(),
            metadata: None,
        };
        for (key, m) in modules {
            state.modules.insert((*key).to_owned(), m.clone());
        }
        state
    }

    #[test]
    fn test_registered_module_resolves_to_itself() {
        let state = state_with(&[("m", module(ModuleStatus::Pending))]);
        let index = HostIndex::build(&state);
        assert_eq!(
            index.resolve("m"),
            HostResolution::Resolved(&"m".to_owned())
        );
    }

    #[test]
    fn test_group_member_resolves_to_representative() {
        let mut group = module(ModuleStatus::Translating);
        group.member_files = Some(vec!["grp".to_owned(), "file:helper.ts".to_owned()]);
        let state = state_with(&[("grp", group)]);
        let index = HostIndex::build(&state);
        assert_eq!(
            index.resolve("file:helper.ts"),
            HostResolution::Resolved(&"grp".to_owned())
        );
    }

    #[test]
    fn test_representative_listing_itself_is_not_ambiguous() {
        // 回归：`populate-modules` 落的 `member_files` 含组代表自身（`DecompUnit.members`
        // 首个成员即代表 key）。把自引用也计为一个宿主，则**每个正常 composite 组**的代表
        // 都会被判成跨组破坏——这是本次修复最容易踩的反向坑。
        let mut group = module(ModuleStatus::Translating);
        group.member_files = Some(vec!["grp".to_owned(), "file:helper.ts".to_owned()]);
        let state = state_with(&[("grp", group)]);
        let index = HostIndex::build(&state);
        assert_eq!(
            index.resolve("grp"),
            HostResolution::Resolved(&"grp".to_owned()),
            "组代表把自己列进 member_files 是正常落盘形态，不是跨组破坏"
        );
        assert!(
            index.broken_partitions().is_empty(),
            "正常 composite 组不得报破坏"
        );
    }

    #[test]
    fn test_member_of_two_groups_is_ambiguous() {
        let mut g1 = module(ModuleStatus::Done);
        g1.member_files = Some(vec!["g1".to_owned(), "file:shared.ts".to_owned()]);
        let mut g2 = module(ModuleStatus::Translating);
        g2.member_files = Some(vec!["g2".to_owned(), "file:shared.ts".to_owned()]);
        let state = state_with(&[("g1", g1), ("g2", g2)]);
        let index = HostIndex::build(&state);
        let g1_key = "g1".to_owned();
        let g2_key = "g2".to_owned();
        assert_eq!(
            index.resolve("file:shared.ts"),
            HostResolution::Ambiguous(vec![&g1_key, &g2_key])
        );
    }

    #[test]
    fn test_registered_module_also_listed_by_another_group_is_ambiguous() {
        // 本次修复的核心形态（MDR-023）：早返回让它判成合法 `Resolved`。
        let shared = module(ModuleStatus::Done);
        let mut g1 = module(ModuleStatus::Translating);
        g1.member_files = Some(vec!["g1".to_owned(), "file:shared.ts".to_owned()]);
        let state = state_with(&[("file:shared.ts", shared), ("g1", g1)]);
        let index = HostIndex::build(&state);
        let shared_key = "file:shared.ts".to_owned();
        let g1_key = "g1".to_owned();
        assert_eq!(
            index.resolve("file:shared.ts"),
            HostResolution::Ambiguous(vec![&shared_key, &g1_key]),
            "既是登记模块、又被别组列为成员 → 互斥被破坏，宿主须两者都在"
        );
    }

    #[test]
    fn test_duplicate_member_entry_is_not_a_broken_partition() {
        // 回归（类型设计视角抓出，本 PR 引入的功能回归）：同一组的 `member_files` 里**重复列**
        // 同一文件时，宿主其实唯一，不是破坏。
        //
        // 被删的两份旧实现用 `.filter(|(_, m)| ...any(...))` 按**模块**筛，每模块最多贡献一条，
        // 天然去重；索引改成按**条目**遍历后这个隐式保证丢了 → 同一宿主入表两次 →
        // `Ambiguous(["grp","grp"])`。后果是对合法划分硬错：四条按 key 归一的命令全拒，
        // `check_blocked_modules` 把它计入 `unresolved` 致 `--auto-unblock` 永不放行（迁移卡死），
        // 而告警文案「若宿主清单里含它自己……二者留一个」在两个宿主同名时根本无法照做。
        //
        // 可达性：`populate-modules` 从图派生（成员已排序去重）不产生重复，但手工编辑与旧文件
        // 会——而「旧文件须可读」正是本模块 doc 自己声明要支持的场景，且全仓没有任何
        // `member_files` 去重校验。
        let mut group = module(ModuleStatus::Translating);
        group.member_files = Some(vec![
            "grp".to_owned(),
            "file:a.ts".to_owned(),
            "file:a.ts".to_owned(),
        ]);
        let state = state_with(&[("grp", group)]);
        let index = HostIndex::build(&state);
        assert_eq!(
            index.resolve("file:a.ts"),
            HostResolution::Resolved(&"grp".to_owned()),
            "重复条目不改变宿主个数，划分是合法的"
        );
        assert!(
            index.broken_partitions().is_empty(),
            "重复列名不得被报成跨组破坏"
        );
    }

    #[test]
    fn test_self_listing_does_not_mask_a_real_cross_group_break() {
        // 排除自引用**不掩盖**真实破坏：`grp` 自列于 `member_files`、同时被 `g2` 列为成员时，
        // 「它是登记模块」那一支已把 `grp` 计为一个宿主，`g2` 再加一个 → 仍判 Ambiguous。
        // 这条推理是自引用排除得以成立的前提，故须有测试钉住。
        let mut grp = module(ModuleStatus::Translating);
        grp.member_files = Some(vec!["grp".to_owned(), "file:a.ts".to_owned()]);
        let mut g2 = module(ModuleStatus::Pending);
        g2.member_files = Some(vec!["g2".to_owned(), "grp".to_owned()]);
        let state = state_with(&[("grp", grp), ("g2", g2)]);
        let index = HostIndex::build(&state);
        let grp_key = "grp".to_owned();
        let g2_key = "g2".to_owned();
        assert_eq!(
            index.resolve("grp"),
            HostResolution::Ambiguous(vec![&g2_key, &grp_key]),
            "自引用排除不得掩盖别组把它列为成员这一真实破坏"
        );
    }

    #[test]
    fn test_unknown_key_is_missing() {
        let state = state_with(&[("m", module(ModuleStatus::Pending))]);
        let index = HostIndex::build(&state);
        assert_eq!(index.resolve("file:ghost.ts"), HostResolution::Missing);
    }

    #[test]
    fn test_broken_partitions_covers_both_forms_and_is_sorted() {
        // 两种破坏形态同时存在，且按文件字典序输出（`modules` 是 HashMap，不排序则漂移）。
        let shared = module(ModuleStatus::Done);
        let mut g1 = module(ModuleStatus::Translating);
        g1.member_files = Some(vec!["g1".to_owned(), "file:shared.ts".to_owned()]);
        let mut g2 = module(ModuleStatus::Pending);
        g2.member_files = Some(vec!["g2".to_owned(), "file:both.ts".to_owned()]);
        let mut g3 = module(ModuleStatus::Pending);
        g3.member_files = Some(vec!["g3".to_owned(), "file:both.ts".to_owned()]);
        let state = state_with(&[
            ("file:shared.ts", shared),
            ("g1", g1),
            ("g2", g2),
            ("g3", g3),
        ]);
        let index = HostIndex::build(&state);
        let broken = index.broken_partitions();
        let got: Vec<(&str, Vec<&str>)> = broken
            .iter()
            .map(|(f, hosts)| (*f, hosts.iter().map(|h| h.as_str()).collect()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("file:both.ts", vec!["g2", "g3"]),
                ("file:shared.ts", vec!["file:shared.ts", "g1"]),
            ]
        );
    }

    #[test]
    fn test_broken_partitions_empty_when_partition_is_valid() {
        // 反向不误报：合法划分（含单文件模块 + 正常组）必须一条都不报。
        let mut group = module(ModuleStatus::Pending);
        group.member_files = Some(vec!["grp".to_owned(), "file:helper.ts".to_owned()]);
        let state = state_with(&[("grp", group), ("solo", module(ModuleStatus::Pending))]);
        let index = HostIndex::build(&state);
        assert!(index.broken_partitions().is_empty());
    }
}
