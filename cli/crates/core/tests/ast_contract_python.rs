//! Python grammar 契约测试。
//!
//! 与 `ast_contract.rs`（TypeScript）同模式：固化 `python.rs` 依赖的
//! tree-sitter-python 节点类型 (kind) 和字段 (field)，grammar 升级时
//! 若重命名/移除会先红于此，而非让集成测试以间接方式暴露。
//!
//! 维护：`python.rs` 新依赖一个 kind/field 时，往 [`CONTRACTS`] 加一行、
//! 并确保 [`SRC`] 能触发它。

use tree_sitter::{Node, Parser};

struct Contract {
    kind: &'static str,
    required_fields: &'static [&'static str],
}

const CONTRACTS: &[Contract] = &[
    // 顶层定义
    Contract {
        kind: "function_definition",
        required_fields: &["name", "body"],
    },
    Contract {
        kind: "class_definition",
        required_fields: &["name", "body"],
    },
    // import
    Contract {
        kind: "import_statement",
        required_fields: &[],
    },
    Contract {
        kind: "import_from_statement",
        required_fields: &[],
    },
    // 控制流（detect_tier 扫描）
    Contract {
        kind: "try_statement",
        required_fields: &["body"],
    },
    Contract {
        kind: "global_statement",
        required_fields: &[],
    },
    Contract {
        kind: "nonlocal_statement",
        required_fields: &[],
    },
    // 装饰器
    Contract {
        kind: "decorator",
        required_fields: &[],
    },
    // 调用表达式
    Contract {
        kind: "call",
        required_fields: &["function", "arguments"],
    },
    // 赋值
    Contract {
        kind: "assignment",
        required_fields: &[],
    },
    // 表达式语句
    Contract {
        kind: "expression_statement",
        required_fields: &[],
    },
    // async 函数（detect_tier 中 "async" 关键字扫描依赖 async 节点位于函数定义内）
    Contract {
        kind: "identifier",
        required_fields: &[],
    },
    // decorated_definition（顶层装饰器包裹函数/类）
    Contract {
        kind: "decorated_definition",
        required_fields: &[],
    },
    // keyword_argument（metaclass=... 检测）
    Contract {
        kind: "keyword_argument",
        required_fields: &["name"],
    },
    // 顶层控制流
    Contract {
        kind: "for_statement",
        required_fields: &["body"],
    },
    Contract {
        kind: "while_statement",
        required_fields: &["body"],
    },
    Contract {
        kind: "if_statement",
        required_fields: &[],
    },
    Contract {
        kind: "with_statement",
        required_fields: &["body"],
    },
    // 属性访问
    Contract {
        kind: "attribute",
        required_fields: &["object", "attribute"],
    },
];

/// 覆盖 [`CONTRACTS`] 全部 kind 的 Python 源码。
const SRC: &str = r#"
import os
from pathlib import Path

counter = 0

@staticmethod
def add(a: int, b: int) -> int:
    return a + b

class Base:
    pass

class Meta(metaclass=type):
    pass

class Derived(Base):
    def method(self):
        self.value = os.path.join("a", "b")

def risky():
    global counter
    try:
        x = int("abc")
    except ValueError:
        pass

def use_nonlocal():
    x = 0
    def nested():
        nonlocal x
        x += 1
    nested()

for i in range(10):
    pass

while False:
    break

if counter > 0:
    pass

with open("f") as fh:
    pass

result = eval("1+1")

async def fetch():
    await asyncio.sleep(1)
"#;

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
fn ast_contract_python_holds() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::language())
        .expect("加载 tree-sitter-python 语法失败");
    let tree = parser.parse(SRC, None).expect("解析测试源码失败");
    let root = tree.root_node();

    for c in CONTRACTS {
        let node = find_first_kind(root, c.kind).unwrap_or_else(|| {
            panic!(
                "kind `{}` 未在 AST 中出现：grammar 可能重命名/移除了它，\
                 或 SRC 未覆盖它（python.rs 仍硬编码依赖该 kind）",
                c.kind
            )
        });
        for field in c.required_fields {
            assert!(
                node.child_by_field_name(field).is_some(),
                "kind `{}` 的 required field `{}` 取不到：grammar 可能改了字段名/结构，\
                 python.rs 中 child_by_field_name(\"{}\") 将静默失效",
                c.kind,
                field,
                field
            );
        }
    }
}
