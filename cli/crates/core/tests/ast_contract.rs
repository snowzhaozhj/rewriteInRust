//! 防 grammar 漂移契约测试。
//!
//! `src/lang/typescript.rs` 硬编码依赖一组 tree-sitter-typescript 的节点类型
//! (kind) 和字段 (field)，这些字符串没有编译期检查。本测试把这些依赖固化成
//! 契约表 [`CONTRACTS`]，解析一段覆盖全部目标结构的 TS 源码，逐项断言每个 kind
//! 存在、每个 required field 可取。
//!
//! tree-sitter-typescript 升级若重命名/移除了某个 kind 或 field，本测试会先红、
//! 并指名道姓哪一项失效——而非让 `tree_sitter_precision` 测试以「F1 下降」间接、
//! 滞后地暴露。实测 0.20.6 → 0.23.2 这些核心节点的 field 零变化，故本测试预期
//! 长期为绿，仅作升级时的明确信号。
//!
//! 维护：`typescript.rs` 新依赖一个 kind/field 时，往 [`CONTRACTS`] 加一行、
//! 并确保 [`SRC`] 能触发它。契约表本身即「本文件依赖哪些 AST 结构」的活文档。
//!
//! 字段断言原则：只列「代码经 `child_by_field_name` 实际取用，且 node-types.json
//! 标 `required=true`」的 field。可选 field（如 `import_specifier.alias`、
//! `import_statement.source`）在某些语法形态下本就可能缺失，断言其存在会误报，
//! 故只验对应 kind 存在、不验该 field。

use tree_sitter::{Node, Parser};

/// `typescript.rs` 依赖的 (kind, 必需 field) 契约。
struct Contract {
    kind: &'static str,
    required_fields: &'static [&'static str],
}

const CONTRACTS: &[Contract] = &[
    // 符号声明
    Contract {
        kind: "function_declaration",
        required_fields: &["name"],
    },
    Contract {
        kind: "generator_function_declaration",
        required_fields: &["name"],
    },
    Contract {
        kind: "class_declaration",
        required_fields: &["name", "body"],
    },
    Contract {
        kind: "abstract_class_declaration",
        required_fields: &["name", "body"],
    },
    Contract {
        kind: "interface_declaration",
        required_fields: &["name"],
    },
    Contract {
        kind: "enum_declaration",
        required_fields: &["name"],
    },
    Contract {
        kind: "type_alias_declaration",
        required_fields: &["name"],
    },
    // 类成员
    Contract {
        kind: "method_definition",
        required_fields: &["name"],
    },
    Contract {
        kind: "public_field_definition",
        required_fields: &["name"],
    },
    // 变量绑定（箭头函数常量）
    Contract {
        kind: "lexical_declaration",
        required_fields: &[],
    },
    Contract {
        kind: "variable_declaration",
        required_fields: &[],
    },
    Contract {
        kind: "variable_declarator",
        required_fields: &["name"],
    },
    // import：source 为 optional，故不断言
    Contract {
        kind: "import_statement",
        required_fields: &[],
    },
    Contract {
        kind: "import_clause",
        required_fields: &[],
    },
    Contract {
        kind: "named_imports",
        required_fields: &[],
    },
    Contract {
        kind: "import_specifier",
        required_fields: &["name"],
    },
    Contract {
        kind: "namespace_import",
        required_fields: &[],
    },
    // export：declaration 为 optional，故不断言
    Contract {
        kind: "export_statement",
        required_fields: &[],
    },
    Contract {
        kind: "export_clause",
        required_fields: &[],
    },
    Contract {
        kind: "export_specifier",
        required_fields: &["name"],
    },
    Contract {
        kind: "namespace_export",
        required_fields: &[],
    },
    // 继承：class_heritage → *_clause → 类型节点
    Contract {
        kind: "class_heritage",
        required_fields: &[],
    },
    Contract {
        kind: "extends_clause",
        required_fields: &[],
    },
    Contract {
        kind: "implements_clause",
        required_fields: &[],
    },
    Contract {
        kind: "generic_type",
        required_fields: &["name"],
    },
    Contract {
        kind: "nested_type_identifier",
        required_fields: &["name"],
    },
    // 调用 / 构造 / 成员访问
    Contract {
        kind: "call_expression",
        required_fields: &["function", "arguments"],
    },
    Contract {
        kind: "new_expression",
        required_fields: &["constructor"],
    },
    Contract {
        kind: "member_expression",
        required_fields: &["object", "property"],
    },
];

/// 覆盖 [`CONTRACTS`] 全部 kind 的 TS 源码。改契约表时同步确保这里能触发新 kind。
///
/// 两个构造时易踩的 grammar 不对称（决定了下面为何这样写）：
/// - `generic_type` 只在 **implements** 的泛型下出现（`implements IFace<number>`）；
///   `extends Base<number>` 的泛型走 `extends_clause` 的 value+type_arguments 字段，
///   不产生 `generic_type`。
/// - `nested_type_identifier` 只在**类型**位置出现（implements / 类型注解）；
///   `extends ns.Base` 的 `ns.Base` 处于**表达式**位置，解析为 `member_expression`。
///   故 `ns.Base` 放在 implements 列表里触发。
const SRC: &str = r#"
import Def, { a as b } from "./m";
import * as ns from "./n";
import "./side-effect";
export { b };
export default Def;
export * as star from "./r";
export const arrow = () => obj.call(new Ctor());
function foo() {}
function* gen() {}
var legacy = 1;
type Alias = string;
interface IFace {}
enum Color { Red }
abstract class Base<T> {}
class Impl extends Base<number> implements IFace<number>, ns.Base {
    data = 0;
    handler = () => {};
    method() {}
}
"#;

/// 深度优先查找首个匹配 `kind` 的节点。required field 在该 kind 的每个实例上都存在，
/// 故首个实例即可代表整类。
fn find_first_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    if node.kind() == kind {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_first_kind(child, kind) {
            return Some(found);
        }
    }
    None
}

#[test]
fn ast_contract_holds() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::language_typescript())
        .expect("加载 tree-sitter-typescript 语法失败");
    let tree = parser.parse(SRC, None).expect("解析测试源码失败");
    let root = tree.root_node();

    for c in CONTRACTS {
        let node = find_first_kind(root, c.kind).unwrap_or_else(|| {
            panic!(
                "kind `{}` 未在 AST 中出现：grammar 可能重命名/移除了它，\
                 或 SRC 未覆盖它（typescript.rs 仍硬编码依赖该 kind）",
                c.kind
            )
        });
        for field in c.required_fields {
            assert!(
                node.child_by_field_name(field).is_some(),
                "kind `{}` 的 required field `{}` 取不到：grammar 可能改了字段名/结构，\
                 typescript.rs 中 child_by_field_name(\"{}\") 将静默失效",
                c.kind,
                field,
                field
            );
        }
    }
}
