//! Rust 项目骨架生成。
//!
//! 委托 `cargo init` 生成标准项目结构，避免硬编码模板。

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{MigrateError, Result};
use crate::process::{run_with_timeout, CARGO_TIMEOUT};

/// 生成 Rust lib 项目骨架。
///
/// 委托 `cargo init --lib` 生成标准结构（Cargo.toml + src/lib.rs）。
/// 如果目标目录已有 Cargo.toml 则跳过（幂等）。
///
/// 返回**警告列表**（非空时调用方须降级 `status=warning` 并如实转达），全部来自
/// [`warn_if_target_is_workspace_member`]：目标成了外层 workspace 的成员，或成员关系
/// 无法判定。
pub fn scaffold_project(name: &str, target_dir: &Path) -> Result<Vec<String>> {
    if name.is_empty() {
        return Err(MigrateError::Config("项目名不能为空".to_string()));
    }

    // 已 scaffold（Cargo.toml 在）时仍确保 .gitignore——首次 cargo init 成功但
    // write_gitignore 失败（权限/磁盘/进程中断）后重跑须能补齐，否则 target/ 会漏进提交
    // （codex 审查指出的失败重试语义漏洞）。
    //
    // 告警同样要出：判据是「状态」，重跑时目标可能已经是 member（上次运行加进去的、
    // 或被 glob 覆盖），用户有权知道——异构交叉 imp3 实证过「首次报 IO 错误、重跑报
    // status:ok 零 warning」会让改动永久隐形。
    if target_dir.join("Cargo.toml").exists() {
        write_gitignore(target_dir)?;
        return Ok(warn_if_target_is_workspace_member(target_dir));
    }

    std::fs::create_dir_all(target_dir)?;

    let output = run_with_timeout(
        Command::new("cargo")
            .args(["init", "--lib", "--name", name, "--vcs", "none"])
            .arg(target_dir),
        CARGO_TIMEOUT,
        "cargo init --lib",
    )
    .map_err(|e| match e {
        MigrateError::Io(io_err) => MigrateError::Config(format!("cargo init 执行失败: {io_err}")),
        other => other,
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MigrateError::Config(format!("cargo init 失败: {stderr}")));
    }

    // 先算告警再写 .gitignore：`write_gitignore` 用 `?` 早退时目标已经建好、可能已成为
    // member，告警若在其后计算就随错误一起丢了（异构交叉 imp3）。重跑会走上面的早返回
    // 路径重新判定，故不会永久丢失，但当次调用也该如实报出。
    let warnings = warn_if_target_is_workspace_member(target_dir);

    write_gitignore(target_dir)?;

    Ok(warnings)
}

/// 相对路径拼上当前工作目录，并消解 `.` / `..` 段。
///
/// **必须绝对化**：`cargo metadata` 要在目标目录下执行、比对的成员路径也是绝对的；而
/// `--target` 常给相对路径（默认值就是 `rust`）。此外相对路径无法与 metadata 返回的绝对
/// 成员路径直接比较。
///
/// 不用 `canonicalize`：它要求路径**已存在**（`..` 之上的中间层不保证存在），且会解析
/// 符号链接、把告警里的路径换成用户不认识的真实路径。这里只做词法消解，够用且无 IO 依赖。
fn absolutize(path: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };

    let mut out = PathBuf::new();
    for part in joined.components() {
        match part {
            std::path::Component::ParentDir => {
                // 弹一层；已到根则忽略（`/..` 就是 `/`）。
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// 目标 crate 是否已成为某个外层 workspace 的成员；是则产出一条警告。
///
/// **判据是「状态」而非「变化量」**——这是本检测的核心设计，两轮审查各自推翻过更朴素的
/// 方案，理由都必须记住：
///
/// 1. 不能匹配 `cargo init` 的 stderr 文案（`Adding ... as member of workspace`）：文案随
///    cargo 版本变动、且可能被本地化。
/// 2. **更不能比对父 manifest 的改动前后内容**——主审实证的结构性盲区：父 workspace 写
///    `members = ["crates/*"]`（glob）时，cargo **不改** manifest，新 crate 却自动成为
///    member。内容比对判据在这类仓库里**永远不可能触发**，而危害照旧（编排器实测：往新
///    crate 塞 `compile_error!` 后父仓 `cargo build` 立即变红）。glob 不是罕见写法，
///    `~/workspace/explore` 下的 oxc 就在用。
///
/// 故直接问 `cargo metadata`：它自己解析 `members`/glob/`exclude`/`default-members`，是
/// workspace 成员关系的权威真值源。目标出现在 `workspace_members` 即命中——这一个判据同时
/// 覆盖显式 members、glob，以及「上次运行已把它加进去、用户没看到告警」的重跑场景
/// （异构交叉 imp3：首次 `cargo init` 成功但 `write_gitignore` 失败 → 整命令以 IO 错误退出，
/// 重跑若沉默用户永远不知道构建配置被改过）。
///
/// `cargo metadata` 失败时**不静默**——分两种情况，靠「上溯路径里是否存在任何
/// `Cargo.toml`」区分（不靠 stderr 文案：裸目录与坏成员**都是 exit 101**，文案又随版本
/// 变动、可被本地化，正是本 PR 反复排除的那类脆弱判据）：
/// - 上溯无任何 `Cargo.toml` → 目标不在任何 Cargo 项目内（裸目录 scaffold，最常见的正常
///   情况），无 workspace 可牵连，**不告警**。
/// - 存在 `Cargo.toml` 但 metadata 仍失败（实测：workspace 里已有语法坏的成员 → exit 101）
///   → 用户的 `cargo build` 本来就是坏的（非迁移产物造成），但**检测确实没能进行**，故如实
///   报「无法判定」，不让调用方以为已确认无事。
///
/// 不报错只告警：成为 member 本身未破坏任何东西，用户也可能确实想要（把迁移产物纳入
/// workspace 是合理意图）；能否接受由用户判断，CLI 的职责是不让它静默发生。
fn warn_if_target_is_workspace_member(target_dir: &Path) -> Vec<String> {
    let absolute_target = absolutize(target_dir);
    let Some(metadata) = workspace_metadata(&absolute_target) else {
        // 目标自身的 Cargo.toml 不算——它是本次产物；只看祖先目录。
        let inside_cargo_project = absolute_target
            .parent()
            .into_iter()
            .flat_map(Path::ancestors)
            .any(|dir| dir.join("Cargo.toml").is_file());
        if !inside_cargo_project {
            return Vec::new();
        }
        return vec![format!(
            "无法判定本目标是否落入某个外层 Rust workspace：目标位于一个 Cargo 项目内，\
             但 `cargo metadata` 执行失败（常见原因是该 workspace 已有成员的 `Cargo.toml` \
             语法有误）。若它确实落在已有 workspace 内，该仓库的 `cargo build`/`cargo test` \
             可能开始连带编译迁移产物——请手工确认 workspace 根的 `members`/`exclude`"
        )];
    };

    // 比对前把两侧都过 canonicalize：`cargo metadata` 返回的是符号链接解析后的真实路径
    // （macOS 上 `/tmp/x` → `/private/tmp/x`），而 `absolutize` 只做词法消解——直接比字符串
    // 会在任何含符号链接的路径下漏报（本仓测试用的 TempDir 就在 `/var` → `/private/var`
    // 下，最初的实现正是在此静默失效）。canonicalize 失败则退回词法路径，至少不 panic。
    let canonical_target = canonicalize_or_self(&absolute_target);

    // workspace_root == 目标自身时不算「外层」——目标就是它自己的 workspace 根，
    // 没有别人的构建配置被牵连。
    if canonicalize_or_self(&metadata.root) == canonical_target {
        return Vec::new();
    }
    let is_member = metadata
        .member_paths
        .iter()
        .any(|p| canonicalize_or_self(p) == canonical_target);
    if !is_member {
        return Vec::new();
    }

    vec![format!(
        "本目标已是外层 workspace（根 {}）的成员——该仓库的 `cargo build --workspace`/\
         `cargo test --workspace` 会连带编译迁移产物（未配 `default-members` 时，裸 \
         `cargo build`/`cargo test` 同样会），而迁移中的 crate 常处于不可编译的中间态\
         （`unimplemented!()`、`TODO(port)`），足以让原本通过的构建开始失败。若不需要，\
         请在 workspace 根的 `Cargo.toml` 里把本 crate 从 `members` 移除**并**加入 \
         `exclude`（仅移除 members 不够——被 glob 覆盖或位于 workspace 目录树内时 cargo \
         仍报 `current package believes it's in a workspace when it's not`），\
         或改用仓库外的 `--target` 路径",
        metadata.root.display()
    )]
}

/// `cargo metadata` 里与成员关系有关的部分。
struct WorkspaceMetadata {
    root: PathBuf,
    /// 各成员 crate 的**目录**绝对路径。
    member_paths: Vec<PathBuf>,
}

/// 解析符号链接用于路径比对；失败（路径不存在等）则原样返回。
///
/// 只用于**比较**，不用于展示——告警里给的是 `absolutize` 的结果，那是用户输入的形态、
/// 更容易认。
fn canonicalize_or_self(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// 在 `dir` 处调用 `cargo metadata`，取 workspace 根与成员目录。
///
/// `--no-deps` 避免解析依赖图——实测这让它不受「依赖不可解析」影响（`[dependencies]` 里写
/// 一个不存在的 crate，metadata 仍 exit 0），也不需要网络。
///
/// 失败返回 `None`；**调用方须区分「无 Cargo 项目」与「真失败」**，不可一律当作无告警
/// （见 [`warn_if_target_is_workspace_member`]）。
fn workspace_metadata(dir: &Path) -> Option<WorkspaceMetadata> {
    let output = run_with_timeout(
        Command::new("cargo")
            .args(["metadata", "--format-version", "1", "--no-deps"])
            .current_dir(dir),
        CARGO_TIMEOUT,
        "cargo metadata",
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let root = PathBuf::from(json.get("workspace_root")?.as_str()?);

    // packages[].manifest_path 给的是 `<dir>/Cargo.toml`，取其父目录即 crate 目录。
    // 只保留 workspace_members 里的包（`--no-deps` 下 packages 已等同成员，但显式过滤
    // 更稳）。workspace_members 的 id 形如 `path+file:///abs/path#name@version`，
    // 解析 id 易随 cargo 版本变动，故改用 manifest_path 对齐。
    let members: Vec<String> = json
        .get("workspace_members")?
        .as_array()?
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    let member_paths = json
        .get("packages")?
        .as_array()?
        .iter()
        .filter(|pkg| {
            pkg.get("id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|id| members.iter().any(|m| m == id))
        })
        .filter_map(|pkg| {
            let manifest = pkg.get("manifest_path")?.as_str()?;
            Path::new(manifest).parent().map(Path::to_path_buf)
        })
        .collect();

    Some(WorkspaceMetadata { root, member_paths })
}

/// 确保 crate 级 `.gitignore` 忽略 `/target`。
///
/// `cargo init --vcs none` 不生成 `.gitignore`；即便用 `--vcs git`，cargo 在检测到
/// 外层已是 git 仓库时也会静默跳过。而并行编排在各 worktree 内跑 `cargo check` 自检
/// 会产生 `target/`，若无 `.gitignore` 则被 `git add -A` 吞进提交、污染合并（M4-ORCH-01
/// PR-5 演练撞出）。故显式确保，不依赖 cargo 的条件行为。
///
/// 后置条件式幂等（而非「文件存在即跳过」）：
/// - 无 `.gitignore` → 新建，写 `/target`。
/// - 有 `.gitignore` 但无有效 `/target` 规则 → 追加一行 `/target`，保留用户既有内容。
/// - 已有有效 `/target` 规则 → 不动。
///
/// 「有效规则」指非注释、去空白后恰为 `/target` 的行——避免把 `#/target`、`/target-old`
/// 等误判为已忽略（codex 审查指出）。
fn write_gitignore(target_dir: &Path) -> Result<()> {
    let path = target_dir.join(".gitignore");
    let existing = match std::fs::read_to_string(&path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };

    let already_ignored = existing
        .as_deref()
        .is_some_and(|content| content.lines().map(str::trim).any(|line| line == "/target"));
    if already_ignored {
        return Ok(());
    }

    match existing {
        // 追加：保留用户既有内容，末尾无换行时先补一个再加规则。
        Some(mut content) => {
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str("/target\n");
            std::fs::write(&path, content)?;
        }
        None => std::fs::write(&path, "/target\n")?,
    }
    Ok(())
}

/// 生成带有 bin target 的 Rust 项目骨架。
///
/// 委托 `cargo init` 生成（默认包含 src/main.rs）。
/// 如果目标目录已有 Cargo.toml 则跳过（幂等）。
///
/// 返回警告列表，语义同 [`scaffold_project`]。
pub fn scaffold_project_with_bin(name: &str, target_dir: &Path) -> Result<Vec<String>> {
    if name.is_empty() {
        return Err(MigrateError::Config("项目名不能为空".to_string()));
    }

    // 见 scaffold_project：已有 Cargo.toml 仍确保 .gitignore（失败重试补齐）+ 重新判定告警。
    if target_dir.join("Cargo.toml").exists() {
        write_gitignore(target_dir)?;
        return Ok(warn_if_target_is_workspace_member(target_dir));
    }

    std::fs::create_dir_all(target_dir)?;

    let output = run_with_timeout(
        Command::new("cargo")
            .args(["init", "--name", name, "--vcs", "none"])
            .arg(target_dir),
        CARGO_TIMEOUT,
        "cargo init",
    )
    .map_err(|e| match e {
        MigrateError::Io(io_err) => MigrateError::Config(format!("cargo init 执行失败: {io_err}")),
        other => other,
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MigrateError::Config(format!("cargo init 失败: {stderr}")));
    }

    // 见 scaffold_project：先算告警再写 .gitignore，否则后者失败时当次告警随错误丢失。
    let warnings = warn_if_target_is_workspace_member(target_dir);

    write_gitignore(target_dir)?;

    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// 在指定目录下执行闭包，结束后恢复原 cwd。
    ///
    /// cwd 是**进程级**状态，而 cargo nextest 默认多线程跑同一进程内的测试——改 cwd 的
    /// 测试之间、以及与 `cargo init` 子进程（继承 cwd）之间会竞态，故串行化。仿 cli_e2e
    /// 的同名 helper：用 `catch_unwind` 保证断言失败时 cwd 也能恢复，否则一个失败会污染
    /// 后续所有测试。
    fn with_cwd<T>(dir: &Path, f: impl FnOnce() -> T) -> T {
        static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        std::env::set_current_dir(&original).unwrap();
        match result {
            Ok(v) => v,
            Err(p) => std::panic::resume_unwind(p),
        }
    }

    #[test]
    fn test_scaffold_project_basic() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("my_project");

        scaffold_project("my_project", &target).unwrap();

        assert!(target.join("Cargo.toml").exists());
        assert!(target.join("src/lib.rs").exists());

        let cargo = std::fs::read_to_string(target.join("Cargo.toml")).unwrap();
        assert!(cargo.contains("my_project"));

        // scaffold 须生成含 /target 的 .gitignore（cargo init --vcs none 不生成，
        // 否则并行 worktree 自检产物 target/ 会被 git add 吞入提交，M4-ORCH-01 PR-5）。
        let gitignore = std::fs::read_to_string(target.join(".gitignore")).unwrap();
        assert_eq!(gitignore, "/target\n", "新建 .gitignore 应恰为 /target");
    }

    #[test]
    fn test_scaffold_gitignore_appends_when_target_missing() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("my_project");
        std::fs::create_dir_all(&target).unwrap();
        // 预置无 /target 的自定义 .gitignore：应保留用户内容 + 追加 /target
        // （codex 审查 Important 1：文件存在但不含 /target 时不能跳过）。
        std::fs::write(target.join(".gitignore"), "/custom\n").unwrap();

        scaffold_project("my_project", &target).unwrap();

        let gitignore = std::fs::read_to_string(target.join(".gitignore")).unwrap();
        assert_eq!(
            gitignore, "/custom\n/target\n",
            "既有内容应保留，/target 追加在后"
        );
    }

    #[test]
    fn test_scaffold_gitignore_no_dup_when_target_present() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("my_project");
        std::fs::create_dir_all(&target).unwrap();
        // 已有有效 /target 规则：不得重复追加。
        std::fs::write(target.join(".gitignore"), "/foo\n/target\n/bar\n").unwrap();

        scaffold_project("my_project", &target).unwrap();

        let gitignore = std::fs::read_to_string(target.join(".gitignore")).unwrap();
        assert_eq!(
            gitignore, "/foo\n/target\n/bar\n",
            "已有 /target 规则不应重复追加"
        );
    }

    #[test]
    fn test_scaffold_gitignore_backfilled_when_cargo_exists() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("my_project");
        std::fs::create_dir_all(&target).unwrap();
        // 模拟「首次 cargo init 成功但 write_gitignore 失败」后的残缺态：
        // Cargo.toml 在、.gitignore 缺。重跑须补齐（codex 审查 Important 2：
        // 早返回路径不能跳过 .gitignore 后置条件）。
        std::fs::write(target.join("Cargo.toml"), "# existing").unwrap();

        scaffold_project("my_project", &target).unwrap();

        let gitignore = std::fs::read_to_string(target.join(".gitignore")).unwrap();
        assert_eq!(
            gitignore, "/target\n",
            "已有 Cargo.toml、缺 .gitignore 应补齐"
        );
        // Cargo.toml 不被触碰。
        let cargo = std::fs::read_to_string(target.join("Cargo.toml")).unwrap();
        assert_eq!(cargo, "# existing");
    }

    #[test]
    fn test_scaffold_project_idempotent() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("my_project");

        scaffold_project("my_project", &target).unwrap();

        let cargo_path = target.join("Cargo.toml");
        std::fs::write(&cargo_path, "# custom content").unwrap();

        scaffold_project("my_project", &target).unwrap();

        let cargo = std::fs::read_to_string(&cargo_path).unwrap();
        assert_eq!(cargo, "# custom content");
    }

    #[test]
    fn test_scaffold_project_with_bin() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("my_bin");

        scaffold_project_with_bin("my_bin", &target).unwrap();

        assert!(target.join("Cargo.toml").exists());
        assert!(target.join("src/main.rs").exists());

        let main = std::fs::read_to_string(target.join("src/main.rs")).unwrap();
        assert!(main.contains("fn main()"));

        // 第二处调用点也须生成 .gitignore（否则删掉 scaffold_project_with_bin 的
        // write_gitignore 调用测试不会红——codex 审查指出）。
        let gitignore = std::fs::read_to_string(target.join(".gitignore")).unwrap();
        assert_eq!(gitignore, "/target\n");
    }

    #[test]
    fn test_scaffold_project_empty_name() {
        let tmp = TempDir::new().unwrap();
        let result = scaffold_project("", tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_scaffold_project_with_bin_empty_name() {
        let tmp = TempDir::new().unwrap();
        let result = scaffold_project_with_bin("", tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_scaffold_project_nested_dir() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("a").join("b").join("c");

        scaffold_project("nested", &target).unwrap();

        assert!(target.join("Cargo.toml").exists());
        assert!(target.join("src/lib.rs").exists());
    }

    // -----------------------------------------------------------------------
    // 外层 workspace 检测（#86 记账 TODO ②）
    //
    // 此前全部 scaffold 测试都在裸 tempdir 跑，而用户典型场景恰是「已有 Rust
    // workspace 的仓库里迁模块进来」——那条路径零覆盖、零告警。
    // -----------------------------------------------------------------------

    /// 造一个含 `[workspace]` 的父仓，返回 (tmp, 父 manifest 路径)。
    fn workspace_parent() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let existing = tmp.path().join("crates/existing/src");
        std::fs::create_dir_all(&existing).unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/existing\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("crates/existing/Cargo.toml"),
            "[package]\nname = \"existing\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::write(existing.join("lib.rs"), "").unwrap();
        let manifest = tmp.path().join("Cargo.toml");
        (tmp, manifest)
    }

    #[test]
    fn test_scaffold_warns_when_target_becomes_workspace_member() {
        let (tmp, manifest) = workspace_parent();
        let before = std::fs::read_to_string(&manifest).unwrap();

        let warnings = scaffold_project("probe", &tmp.path().join("crates/probe")).unwrap();

        // cargo 把新 crate 塞进 members——先证明改动真的发生（不然断言告警就是空转）。
        let after = std::fs::read_to_string(&manifest).unwrap();
        assert_ne!(
            after, before,
            "前置假设不成立：cargo 未改动父 manifest（cargo 行为变了？），\
             此时告警断言无意义——请重新核对本测试的前提"
        );
        assert!(
            after.contains("crates/probe"),
            "父 members 应含新 crate: {after}"
        );

        assert_eq!(warnings.len(), 1, "应恰好一条告警: {warnings:?}");
        let w = &warnings[0];
        assert!(
            w.contains("workspace") && w.contains("members"),
            "告警须点明成员关系与 members: {w}"
        );
        // 告警给的是 workspace 根目录（用户据它去找根 Cargo.toml）。
        assert!(
            w.contains(&absolutize(tmp.path()).display().to_string()),
            "告警须给出 workspace 根路径（用户要据此去修）: {w}"
        );
    }

    #[test]
    fn test_scaffold_no_warning_in_bare_dir() {
        // 裸目录（无外层 workspace）：cargo 不会追加 member，不该告警。
        let tmp = TempDir::new().unwrap();
        let warnings = scaffold_project("solo", &tmp.path().join("solo")).unwrap();
        assert!(warnings.is_empty(), "裸目录不应告警: {warnings:?}");
    }

    #[test]
    fn test_scaffold_no_warning_when_parent_is_plain_package() {
        // 父目录有 Cargo.toml 但是普通 [package]（非 workspace）：目标不会成为任何
        // workspace 的成员，不该告警。
        //
        // fixture 必须是**合法可解析**的 package——须带 src/lib.rs：实测缺源文件时 cargo
        // 报 `no targets specified in the manifest`、`cargo metadata` 退出 101，于是走进
        // 「无法判定」分支而误判为回归。真实项目的 package 都有源文件。
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"plain\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/lib.rs"), "").unwrap();
        let before = std::fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();

        let warnings = scaffold_project("inner", &tmp.path().join("inner")).unwrap();

        assert_eq!(
            std::fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap(),
            before,
            "普通 package 父 manifest 不该被改动"
        );
        assert!(
            warnings.is_empty(),
            "非 workspace 父目录不应告警: {warnings:?}"
        );
    }

    #[test]
    fn test_scaffold_with_bin_also_warns_on_workspace_membership() {
        // 两个 scaffold 函数须行为一致——否则删掉其中一个的检测调用，测试不会红。
        let (tmp, _manifest) = workspace_parent();
        let warnings =
            scaffold_project_with_bin("probe_bin", &tmp.path().join("crates/probe_bin")).unwrap();
        assert_eq!(warnings.len(), 1, "with_bin 也应告警: {warnings:?}");
    }

    #[test]
    fn test_scaffold_rerun_still_warns_when_still_a_member() {
        // 重跑走早返回、不调 cargo init，故无「改动前后」可比对——但上一次运行可能已把该
        // crate 塞进父 members 而用户没看到告警（codex imp3：首次 cargo init 成功、随后
        // write_gitignore 失败 → 整命令以 IO 错误退出，重跑再沉默就永远不知道）。故早返回
        // 路径改按**当前状态**判定：members 已含本目标即告警。
        let (tmp, _manifest) = workspace_parent();
        let target = tmp.path().join("crates/probe");

        let first = scaffold_project("probe", &target).unwrap();
        assert_eq!(first.len(), 1, "首次应告警（cargo 改了父 manifest）");

        let second = scaffold_project("probe", &target).unwrap();
        assert_eq!(
            second.len(),
            1,
            "重跑仍应告警——目标确实是 member，用户有权知道: {second:?}"
        );
        assert!(
            second[0].contains("已是外层 workspace"),
            "重跑走早返回路径，判据仍是「当前是否为成员」: {}",
            second[0]
        );
    }

    /// `cargo init` 成功但 `.gitignore` 写失败时，父 manifest 已被改——告警不能随错误丢失。
    ///
    /// codex imp3 实证的原始症状：首次报 IO error（用户不知父 manifest 已被改）、
    /// 移除障碍后重跑报 `status:ok` 零 warning，**两次都不知道**。
    #[test]
    fn test_warning_survives_gitignore_write_failure_via_rerun() {
        let (tmp, _manifest) = workspace_parent();
        let target = tmp.path().join("crates/failmod");

        // 用目录占位 .gitignore，强制 write_gitignore 失败。
        std::fs::create_dir_all(target.join(".gitignore")).unwrap();
        let first = scaffold_project("failmod", &target);
        assert!(first.is_err(), "gitignore 写失败应报错: {first:?}");

        // 此刻 cargo 已经改了父 manifest（这是问题的前提，先证明它）。
        let manifest_content = std::fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
        assert!(
            manifest_content.contains("crates/failmod"),
            "前置假设不成立：cargo 未改父 manifest，本测试无意义: {manifest_content}"
        );

        // 移除障碍后重跑：必须告警，否则用户永远不知道构建配置被改过。
        std::fs::remove_dir(target.join(".gitignore")).unwrap();
        let second = scaffold_project("failmod", &target).unwrap();
        assert_eq!(
            second.len(),
            1,
            "重跑必须告警——否则「首次报 IO 错误 + 重跑报 ok」让改动永久隐形: {second:?}"
        );
    }

    /// 端到端：`[workspace.package]`（无独立 `[workspace]` 段）也要告警。
    #[test]
    fn test_scaffold_warns_on_workspace_package_only_parent() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[workspace.package]\nedition = \"2021\"\n",
        )
        .unwrap();

        let warnings = scaffold_project("probe", &tmp.path().join("crates/probe")).unwrap();

        let after = std::fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
        assert!(
            after.contains("crates/probe"),
            "前置假设不成立：cargo 未把 crate 加进 members: {after}"
        );
        assert_eq!(
            warnings.len(),
            1,
            "`[workspace.package]` 写法也须告警（词法判据在此漏报）: {warnings:?}"
        );
    }

    #[test]
    fn test_absolutize_resolves_dot_segments() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(absolutize(Path::new("a/b")), cwd.join("a/b"));
        assert_eq!(absolutize(Path::new("./a")), cwd.join("a"));
        assert_eq!(absolutize(Path::new("a/../b")), cwd.join("b"));
        // 绝对路径原样消解，不拼 cwd。
        assert_eq!(
            absolutize(Path::new("/x/y/../z")),
            PathBuf::from("/x/z"),
            "绝对路径不应被拼上 cwd"
        );
        // 根之上的 `..` 忽略，不产出空路径。
        assert_eq!(absolutize(Path::new("/../a")), PathBuf::from("/a"));
    }

    #[test]
    fn test_scaffold_warning_path_is_absolute() {
        // 告警里的 workspace 根须绝对——用户要据它去找文件，相对形态没法定位
        // （编排器实测：相对 --target 时告警曾输出 `Cargo.toml` / `../Cargo.toml`）。
        // 本测试同时覆盖「从子目录用相对 --target」这条曾漏报的路径。
        let (tmp, _manifest) = workspace_parent();
        let sub = tmp.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();

        let warnings = with_cwd(&sub, || {
            scaffold_project("rel_mod", Path::new("../crates/rel_mod")).unwrap()
        });

        assert_eq!(warnings.len(), 1, "相对路径下也应告警: {warnings:?}");
        let root = absolutize(tmp.path());
        assert!(
            warnings[0].contains(&root.display().to_string()),
            "告警应含绝对的 workspace 根 {}，实际: {}",
            root.display(),
            warnings[0]
        );
    }

    /// **glob workspace**：`members = ["crates/*"]` 时 cargo **不改** manifest，
    /// 新 crate 却自动成为成员——「比对 manifest 改动」的判据在此结构上永不触发。
    ///
    /// 主审实证的盲区，编排器独立复现：旧判据下 CLI 报 `status:ok` 零 warning，而往新
    /// crate 塞 `compile_error!` 后父仓 `cargo build` 立即变红（危害照旧）。glob 不罕见，
    /// `~/workspace/explore` 下的 oxc 就在用。这是判据从「变化量」改为「状态」的直接理由。
    #[test]
    fn test_scaffold_warns_under_glob_workspace() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("crates/existing/src")).unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("crates/existing/Cargo.toml"),
            "[package]\nname = \"existing\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::write(tmp.path().join("crates/existing/src/lib.rs"), "").unwrap();

        let manifest_before = std::fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap();
        let warnings = scaffold_project("globbed", &tmp.path().join("crates/globbed")).unwrap();

        // 前置假设：cargo 确实**没改** manifest（这正是旧判据失效的原因）。若哪天 cargo
        // 改了行为，这条会红并提醒重新审视——而不是让告警断言静默失去意义。
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("Cargo.toml")).unwrap(),
            manifest_before,
            "前置假设不成立：glob 下 cargo 竟改了 manifest，请重新评估判据选择"
        );
        assert_eq!(
            warnings.len(),
            1,
            "glob 覆盖使目标成为成员，必须告警（比对判据在此永不触发）: {warnings:?}"
        );
    }

    /// 告警须给出可行的处置——仅从 `members` 移除不够，还得 `exclude`。
    ///
    /// 主审实证：照旧文案「请从 members 移除该条目」操作后，cargo 报
    /// `current package believes it's in a workspace when it's not`，用户得到一个编译不了的
    /// crate；而 `scaffolder.md` 又禁止 agent 自行加 `exclude`——文案把用户领进死路。
    #[test]
    fn test_warning_mentions_exclude_not_just_members() {
        let (tmp, _manifest) = workspace_parent();
        let warnings = scaffold_project("probe", &tmp.path().join("crates/probe")).unwrap();

        assert_eq!(warnings.len(), 1, "应告警: {warnings:?}");
        assert!(
            warnings[0].contains("exclude"),
            "处置建议须提到 exclude（仅移除 members 会得到编译不了的 crate）: {}",
            warnings[0]
        );
        // 危害范围须限定到 --workspace：设计契约审查实测，配了 `default-members` 时裸
        // `cargo build`/`cargo test` **不**编译迁移产物，只有 `--workspace` 才会。
        // 原文案「该仓库的 cargo build/test 会连带编译」是过度承诺。
        assert!(
            warnings[0].contains("--workspace"),
            "危害描述须限定到 --workspace（default-members 下裸 build 不编译）: {}",
            warnings[0]
        );
    }

    /// 目标自身就是 workspace 根时不算「外层」——没有别人的构建配置被牵连。
    #[test]
    fn test_no_warning_when_target_is_its_own_workspace_root() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("solo");
        std::fs::create_dir_all(&target).unwrap();
        // 预置一个自带 [workspace] 的 crate（scaffold 会走早返回路径）。
        std::fs::write(
            target.join("Cargo.toml"),
            "[workspace]\n\n[package]\nname = \"solo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(target.join("src")).unwrap();
        std::fs::write(target.join("src/lib.rs"), "").unwrap();

        let warnings = scaffold_project("solo", &target).unwrap();
        assert!(
            warnings.is_empty(),
            "目标即自己的 workspace 根，不该告警: {warnings:?}"
        );
    }

    /// `cargo metadata` 失败但目标确在某个 Cargo 项目内 → 报「无法判定」，不静默。
    ///
    /// 编排器实测：workspace 里已有语法坏的成员时 `cargo metadata` 退出码 101。此时用户的
    /// `cargo build` 本来就坏（非迁移产物造成），但**检测没能进行**，静默返回「无告警」等于
    /// 谎称已确认无事。
    #[test]
    fn test_warns_unknown_when_metadata_fails_inside_cargo_project() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("crates/broken/src")).unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n",
        )
        .unwrap();
        // 语法坏的成员 manifest：让 cargo metadata 失败。
        std::fs::write(
            tmp.path().join("crates/broken/Cargo.toml"),
            "[package\nbad!!!",
        )
        .unwrap();
        std::fs::write(tmp.path().join("crates/broken/src/lib.rs"), "").unwrap();

        let warnings = scaffold_project("newmod", &tmp.path().join("crates/newmod")).unwrap();

        assert_eq!(
            warnings.len(),
            1,
            "metadata 失败且目标在 Cargo 项目内，须报「无法判定」而非静默: {warnings:?}"
        );
        assert!(
            warnings[0].contains("无法判定"),
            "告警须如实说明是判定失败、不是确认无事: {}",
            warnings[0]
        );
    }

    /// 裸目录（上溯无任何 `Cargo.toml`）：metadata 同样失败，但这是正常情况，不该告警。
    ///
    /// 与上一条同为 `cargo metadata` exit 101，故**不能靠 stderr 文案区分**（文案随版本变动、
    /// 可被本地化——正是本 PR 反复排除的脆弱判据），改按「上溯是否存在 Cargo.toml」区分。
    #[test]
    fn test_no_unknown_warning_in_bare_dir_without_cargo_project() {
        let tmp = TempDir::new().unwrap();
        let warnings = scaffold_project("solo", &tmp.path().join("solo")).unwrap();
        assert!(
            warnings.is_empty(),
            "裸目录 scaffold 是正常情况，不该报「无法判定」: {warnings:?}"
        );
    }
}
