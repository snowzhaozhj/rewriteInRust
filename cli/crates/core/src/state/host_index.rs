//! `member_files` 划分的宿主索引：把「一个 key 归属哪个模块」收成唯一实现。
//!
//! # 为什么要有这个模块
//!
//! composite 组折叠后 `modules` 只登记**组代表** key，组内其余成员（`member_files` 里的
//! 文件）不是独立模块。于是「一个 key 属于谁」有两个来源——登记模块自身，以及把它列为
//! 成员的组——而这两个来源**可以同时命中**，那正是 `member_files` 跨组互斥不变量被破坏
//! 的形态之一。
//!
//! 此前 `validate::resolve_blocked_ref` 与 `MigrationStateMachine::canonical_module_key`
//! 各写了一份判定，且两份都是「先查 `modules`，命中就早返回」——于是「key 既是登记模块、
//! 又被别的组列为成员」这一类破坏被判成**合法**引用。实测后果不是漏诊断而是数据损坏：
//! `file:shared.ts` 登记为 `done` 而 `g1` 组（`translating`）把它列为成员时，
//! `validate state` 零跨组告警、`--auto-unblock` 真的解除了引用它的模块并落盘，而该文件
//! 实际还在翻译中（MDR-023）。
//!
//! 故本模块的两条设计约束：
//!
//! 1. **不早返回**。[`HostIndex::resolve`] 把两个来源的宿主**全部收集完**再判个数，
//!    互斥破坏因此无法从任何一侧溜过去。
//! 2. **唯一实现**。validate 侧（降级为告警，旧文件须可读）与 machine 侧（硬错，
//!    MDR-015）处置策略不同，但**判定**共用这里——两份判定必然漂移，而这个功能已经为
//!    「同一概念两份表示」付过账（MDR-021 待办 1 第三点）。

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
/// 建表一次 O(成员总数)，之后每查 O(1)。**不提供「查单条」的自由函数**——那正是让调用方
/// 在循环里退回二次复杂度的入口。单条查询请显式 `build` 再 `resolve`。
pub struct HostIndex<'a> {
    state_file: &'a MigrationStateFile,
    /// 成员文件 → 把它列为成员的模块 key（字典序）。
    ///
    /// **不含「组代表把自己列进 `member_files`」那一项**：`populate-modules` 落盘的
    /// `member_files` 就是全体成员、含代表自身（`DecompUnit.members` 的首个成员即组代表
    /// key），故那不是破坏。它已由 [`resolve`](HostIndex::resolve) 的「自己是登记模块」
    /// 那一支计为一个宿主；这里再计一次，每个正常 composite 组都会被误判成跨组破坏。
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
        for hosts in by_member.values_mut() {
            hosts.sort();
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
