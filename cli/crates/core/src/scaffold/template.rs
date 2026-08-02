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
    // 或被 glob 覆盖），用户有权知道——已实证过「首次报 IO 错误、重跑报
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
    // member，告警若在其后计算就随错误一起丢了（审查实证）。重跑会走上面的早返回
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
/// 不用 `canonicalize`：它要求路径**已存在**（`..` 之上的中间层不保证存在），也不需要 IO。
/// 这里只做词法消解，够用。（注意这**不**意味着告警里的路径是用户输入的形态——告警展示的是
/// `cargo metadata` 返回的 workspace 根，那是符号链接已解析的真实路径，见
/// [`canonicalize_or_self`]。）
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
/// （首次 `cargo init` 成功但 `write_gitignore` 失败 → 整命令以 IO 错误退出，
/// 重跑若沉默用户永远不知道构建配置被改过）。
///
/// `cargo metadata` 失败时**不静默**——分两种情况，靠「上溯路径里是否存在任何
/// `Cargo.toml`」区分（不靠 stderr 文案：无 manifest 与坏 manifest **都是 exit 101**，文案又
/// 随版本变动、可被本地化，正是本 PR 反复排除的那类脆弱判据）：
/// - 上溯无任何 `Cargo.toml` → 目标不在任何 Cargo 项目内，无 workspace 可牵连，**不告警**。
/// - 存在 `Cargo.toml` 但 metadata 仍失败（实测：workspace 里已有语法坏的成员 → exit 101）
///   → 用户的 `cargo build` 本来就是坏的（非迁移产物造成），但**检测确实没能进行**，故如实
///   报「无法判定」，不让调用方以为已确认无事。
///
/// 注意本函数在 `cargo init` **之后**执行，故常见的裸目录 scaffold 走不到失败分支——产出的
/// crate 是合法可解析的，metadata 在其中 exit 0 且 `workspace_root` 指向目标自身，由下方
/// 「目标即自己的 workspace 根」短路返回。失败分支要么是残缺 manifest（幂等重跑路径），
/// 要么是外层 workspace 本身坏了。
///
/// 不报错只告警：成为 member 本身未破坏任何东西，用户也可能确实想要（把迁移产物纳入
/// workspace 是合理意图）；能否接受由用户判断，CLI 的职责是不让它静默发生。
fn warn_if_target_is_workspace_member(target_dir: &Path) -> Vec<String> {
    let absolute_target = absolutize(target_dir);
    let Some(metadata) = workspace_metadata(&absolute_target) else {
        // 上溯前先解符号链接：`absolutize` 只做词法消解，而 `--target` 路径里的符号链接段会让
        // 词法祖先链指向一个不存在的目录树（`/tmp/link/x` 的词法祖先是 `/tmp/link`、`/tmp`，
        // 而真实位置可能是某 workspace 内的 `/tmp/repo/crates/x`）。漏解则该场景判成「裸目录」
        // 静默返回空告警——与成功分支（下方两侧 `canonicalize_or_self`）修的是同一个坑，
        // 类型设计视角实测复现：直路径报 warning、经符号链接的同一目标报 ok 零 warning。
        //
        // 目标自身的 Cargo.toml 不算——它是本次产物；只看祖先目录。
        let inside_cargo_project = canonicalize_or_self(&absolute_target)
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
         `cargo test --workspace` 会连带编译迁移产物（若该 workspace 无根 package 且未配 \
         `default-members`，在 workspace 根执行的裸 `cargo build`/`cargo test` 同样会），\
         而迁移中的 crate 常处于不可编译的中间态（`unimplemented!()`、`TODO(port)`），\
         足以让原本通过的构建开始失败。若不需要，请在 workspace 根的 `Cargo.toml` 里把本 \
         crate 从 `members` 移除**并**加入 `exclude`（仅移除 members 不够——被 glob 覆盖或\
         位于 workspace 目录树内时 cargo 仍报 `current package believes it's in a \
         workspace when it's not`），或改用仓库外的 `--target` 路径",
        metadata.root.display()
    )]
}

/// `cargo metadata` 里与成员关系有关的部分。
struct WorkspaceMetadata {
    root: PathBuf,
    /// 各成员 crate 的**目录**绝对路径。
    member_paths: Vec<PathBuf>,
}

/// 解析符号链接；失败（路径不存在等）则原样返回。
///
/// 两处用途：① 路径**比对**——`cargo metadata` 返回的成员路径已解析符号链接，词法路径与它
/// 直接比会漏报；② 「目标是否在某 Cargo 项目内」的**祖先链上溯**，同理。
///
/// 注意告警文案里展示的 workspace 根取自 `metadata.root`，故也是符号链接**已解析**的形态
/// （用户输入 `crates/probe` 时可能显示 `/private/tmp/...`）——路径仍然有效可用，只是未必是
/// 用户输入的样子。
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

    parse_workspace_metadata(&output.stdout)
}

/// 从 `cargo metadata` 的 JSON 里取 workspace 根与成员目录。
///
/// 与子进程调用分离，便于直接喂合成 JSON 测 schema 漂移（沿用本 PR「不改真实文件、喂合成
/// 字符串」的测试惯例）。
fn parse_workspace_metadata(stdout: &[u8]) -> Option<WorkspaceMetadata> {
    let json: serde_json::Value = serde_json::from_slice(stdout).ok()?;
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
    let member_paths: Vec<PathBuf> = json
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

    // 不变量：`workspace_members` 非空则 `member_paths` 必非空。
    //
    // 上面两个 `filter`/`filter_map` 里的 `id` 与 `manifest_path` 是**内部**吞掉的（不像
    // `workspace_root`/`packages` 那样用 `?` 上抛），任一字段改名或改格式都只会让
    // `member_paths` 静默变空 → `is_member` 恒 false → `status:ok` 零告警，正是本检测要
    // 消灭的失效模式。而 `id` 格式确有先例会变（cargo 1.77 换过 PackageId 格式）。
    //
    // 实测真 cargo 下二者恒等长（workspace 根与成员子目录下各跑均一致），故「members 非空
    // 而 paths 空」只可能是 schema 漂移：返回 `None` 让调用方报「无法判定」，不静默放行。
    if member_paths.is_empty() && !members.is_empty() {
        return None;
    }

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
    /// cwd 是**进程级**状态，而 `cargo test` 在同一进程内多线程跑测试——改 cwd 的测试之间、
    /// 以及与 `cargo init` 子进程（继承 cwd）之间会竞态，故串行化。（`just test` 用的 nextest
    /// 是 process-per-test，本身无此问题；但本仓也支持直接 `cargo test`，需防的是那条路径。）
    /// 仿 cli_e2e 的同名 helper：用 `catch_unwind` 保证断言失败时 cwd 也能恢复，否则一个失败
    /// 会污染后续所有测试。
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
        let target = tmp.path().join("crates/probe_bin");
        let warnings = scaffold_project_with_bin("probe_bin", &target).unwrap();
        assert_eq!(warnings.len(), 1, "with_bin 也应告警: {warnings:?}");

        // 幂等重跑（早返回路径）同样须告警。四个接线点（两函数 × 主路径/早返回）里，
        // 这一处此前无守卫——测试覆盖视角实测：把 `with_bin` 早返回改成 `Ok(Vec::new())`，
        // 23 个测试全绿。语义同 `test_scaffold_rerun_still_warns_when_still_a_member`。
        let rerun = scaffold_project_with_bin("probe_bin", &target).unwrap();
        assert_eq!(
            rerun.len(),
            1,
            "with_bin 重跑走早返回，仍须按当前状态告警: {rerun:?}"
        );
    }

    #[test]
    fn test_scaffold_rerun_still_warns_when_still_a_member() {
        // 重跑走早返回、不调 cargo init，故无「改动前后」可比对——但上一次运行可能已把该
        // crate 塞进父 members 而用户没看到告警（首次 cargo init 成功、随后
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
    /// 已实证的原始症状：首次报 IO error（用户不知父 manifest 已被改）、
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
        // 危害范围须限定到 --workspace：实测裸 build 是否波及迁移产物取决于 workspace 形态
        // ——配了 `default-members`、或该 workspace 有根 package（`[package]` + `[workspace]`）
        // 时，裸 `cargo build`/`cargo test` **不**编译迁移产物；只有虚拟 manifest 且未配
        // `default-members`、且在 ws 根执行时才会。故文案不能无条件说「裸 build 也会」。
        assert!(
            warnings[0].contains("--workspace"),
            "危害描述须限定到 --workspace（根 package 型或配了 default-members 时裸 build 不编译）: {}",
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

    /// `cargo metadata` 的 JSON schema 漂移**不得**让检测静默放行。
    ///
    /// `id` 与 `manifest_path` 是在 `filter`/`filter_map` 内部吞掉的（不像 `workspace_root`/
    /// `packages` 那样用 `?` 上抛），任一字段改名或改格式都只会让 `member_paths` 静默变空
    /// → `is_member` 恒 false → `status:ok` 零告警，正是本检测要消灭的失效模式。而 `id`
    /// 格式确有先例会变（cargo 1.77 换过 PackageId 格式）。
    ///
    /// 喂合成 JSON 而非改真实 cargo 输出：真 cargo 下无法构造这些漂移，且合成串让每种
    /// 漂移都可独立回归。
    #[test]
    fn test_metadata_schema_drift_does_not_silently_pass() {
        let good = br#"{
            "workspace_root": "/ws",
            "workspace_members": ["path+file:///ws/crates/a#a@0.1.0"],
            "packages": [{
                "id": "path+file:///ws/crates/a#a@0.1.0",
                "manifest_path": "/ws/crates/a/Cargo.toml"
            }]
        }"#;
        let parsed = parse_workspace_metadata(good).expect("正常 JSON 须解析成功");
        assert_eq!(parsed.root, PathBuf::from("/ws"));
        assert_eq!(parsed.member_paths, vec![PathBuf::from("/ws/crates/a")]);

        // ① id 格式变化（如 cargo 1.77 那次）→ 交叉过滤全落空。
        let id_drift = br#"{
            "workspace_root": "/ws",
            "workspace_members": ["path+file:///ws/crates/a#a@0.1.0"],
            "packages": [{
                "id": "registry+file:///ws/crates/a#a@0.1.0",
                "manifest_path": "/ws/crates/a/Cargo.toml"
            }]
        }"#;
        assert!(
            parse_workspace_metadata(id_drift).is_none(),
            "id 格式漂移须返回 None（让调用方报「无法判定」），不得静默放行"
        );

        // ② manifest_path 改名 → 取不到 crate 目录。
        let path_drift = br#"{
            "workspace_root": "/ws",
            "workspace_members": ["path+file:///ws/crates/a#a@0.1.0"],
            "packages": [{
                "id": "path+file:///ws/crates/a#a@0.1.0",
                "manifest_path_v2": "/ws/crates/a/Cargo.toml"
            }]
        }"#;
        assert!(
            parse_workspace_metadata(path_drift).is_none(),
            "manifest_path 改名须返回 None，不得静默放行"
        );

        // 反向：成员本就为空（单 package 非 workspace）不该被误判为漂移。
        let genuinely_empty = br#"{
            "workspace_root": "/solo",
            "workspace_members": [],
            "packages": []
        }"#;
        let empty = parse_workspace_metadata(genuinely_empty)
            .expect("members 本就为空是合法状态，不该误报漂移");
        assert!(empty.member_paths.is_empty());
    }

    /// `cargo metadata` 失败**且**上溯无任何 `Cargo.toml` → 不告警（裸目录 scaffold，正常）。
    ///
    /// 这条守的是 `inside_cargo_project` 分流本身。要点在**怎么逼进这个分支**：
    /// `cargo init` 的产物是合法可解析的 crate，metadata 在其中 exit 0（实测
    /// `workspace_root` 就是目标自身），所以普通裸目录用例走的是成功分支的
    /// 「目标即自己的 workspace 根」短路，**根本到不了这里**——测试覆盖视角实测：把
    /// `if !inside_cargo_project { return … }` 整块删掉（即 metadata 一失败就无条件报
    /// 「无法判定」），23 个测试全绿。
    ///
    /// 故这里预置一个残缺的 `Cargo.toml`：它使调用走早返回路径（幂等分支）、且让 metadata
    /// 必然失败，而 `TempDir` 的祖先链上没有任何 `Cargo.toml`，于是判定「不在 Cargo 项目
    /// 内」→ 不告警。与上一条（`..._inside_cargo_project`）恰好是同一分流的两侧。
    #[test]
    fn test_no_unknown_warning_when_metadata_fails_outside_cargo_project() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("solo");
        std::fs::create_dir_all(&target).unwrap();
        // 残缺 manifest：走早返回路径且让 cargo metadata 失败。
        std::fs::write(target.join("Cargo.toml"), "# existing").unwrap();

        // 前置假设：祖先链确实无 Cargo.toml，否则本用例会退化成「无法判定」那一侧。
        assert!(
            !tmp.path().join("Cargo.toml").exists(),
            "TempDir 祖先链须无 Cargo.toml"
        );

        let warnings = scaffold_project("solo", &target).unwrap();

        assert!(
            warnings.is_empty(),
            "metadata 失败但目标不在任何 Cargo 项目内 → 裸目录 scaffold，不该报「无法判定」: {warnings:?}"
        );
    }

    /// `--target` 路径**自身含符号链接段**时，「无法判定」分支仍须命中。
    ///
    /// 这是类型设计视角实证出的漏报：成功分支比对路径时两侧过了 `canonicalize_or_self`，而
    /// metadata 失败分支的祖先链上溯一度走词法路径——`/tmp/link/x` 的词法祖先只有 `/tmp/link`
    /// 和 `/tmp`（都无 `Cargo.toml`），真实位置 `/tmp/repo/crates/x` 却在 workspace 内，于是
    /// 判成「裸目录」静默返回空告警。同一坏 workspace 下直路径报 warning、经符号链接报 ok。
    ///
    /// 与既有的 `TempDir` 用例不重复：那些用例只有**祖先层**符号链接（macOS `/var` →
    /// `/private/var`），`--target` 参数内不含符号链接段，走不到这条路径。
    #[test]
    fn test_warns_unknown_when_target_path_contains_symlink_segment() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(repo.join("crates/broken/src")).unwrap();
        std::fs::write(
            repo.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n",
        )
        .unwrap();
        // 同上一条：语法坏的成员让 cargo metadata 失败，逼入「无法判定」分支。
        std::fs::write(repo.join("crates/broken/Cargo.toml"), "[package\nbad!!!").unwrap();
        std::fs::write(repo.join("crates/broken/src/lib.rs"), "").unwrap();

        // link → repo/crates，故 link/viasym 的真实位置是 repo/crates/viasym（在 workspace 内）。
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(repo.join("crates"), &link).unwrap();

        // 前置假设：目标经符号链接访问，且其词法祖先链上确实没有 Cargo.toml——否则本用例
        // 退化成「直路径」场景、不再覆盖它要防的漏报。
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert!(
            !link.join("Cargo.toml").exists() && !tmp.path().join("Cargo.toml").exists(),
            "词法祖先链须无 Cargo.toml，否则用例覆盖不到符号链接漏报"
        );

        let warnings = scaffold_project("viasym", &link.join("viasym")).unwrap();

        assert_eq!(
            warnings.len(),
            1,
            "目标真实位置在坏 workspace 内，经符号链接访问也须报「无法判定」: {warnings:?}"
        );
        assert!(
            warnings[0].contains("无法判定"),
            "须如实说明判定失败: {}",
            warnings[0]
        );
    }
}
