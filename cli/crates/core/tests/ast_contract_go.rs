//! Go grammar 契约测试。
//!
//! 与 `ast_contract.rs`（TypeScript）/ `ast_contract_python.rs` 同模式：固化
//! `go.rs` 现在及后续（Sprint C PR-C2/C3 的 analyze_file/resolve_import）依赖的
//! tree-sitter-go 节点类型 (kind) 和字段 (field)，grammar 升级时若重命名/移除会先红
//! 于此，而非让集成测试以间接方式暴露。
//!
//! PR-C1 的 `go.rs` 目前仅 `child_by_field_name("path")`（import_spec）一处真正依赖字段，
//! 其余字段断言（function/method/type_spec/call/selector/composite/qualified 等）是给
//! PR-C2/C3 符号/调用/签名提取的**前向登记 guard**——下个 PR 落地即生效，提前锁定避免漂移。
//!
//! 维护：`go.rs` 新依赖一个 kind/field 时，往 [`CONTRACTS`] 加一行、并确保
//! [`SRC`] 能触发它。字段以 tree-sitter-go-0.21 `node-types.json` 为准。

use tree_sitter::{Node, Parser};

struct Contract {
    kind: &'static str,
    required_fields: &'static [&'static str],
}

const CONTRACTS: &[Contract] = &[
    // 包声明（detect_tier 顶层分档忽略）
    Contract {
        kind: "package_clause",
        required_fields: &[],
    },
    // import（detect_tier 扫危险包 reflect/unsafe/C）
    Contract {
        kind: "import_declaration",
        required_fields: &[],
    },
    Contract {
        kind: "import_spec",
        required_fields: &["path"],
    },
    // 顶层声明
    Contract {
        kind: "const_declaration",
        required_fields: &[],
    },
    Contract {
        kind: "const_spec",
        required_fields: &["name"],
    },
    Contract {
        kind: "var_declaration",
        required_fields: &[],
    },
    Contract {
        kind: "var_spec",
        required_fields: &["name"],
    },
    // 类型定义
    Contract {
        kind: "type_declaration",
        required_fields: &[],
    },
    Contract {
        kind: "type_spec",
        required_fields: &["name", "type"],
    },
    Contract {
        kind: "struct_type",
        required_fields: &[],
    },
    Contract {
        kind: "interface_type",
        required_fields: &[],
    },
    Contract {
        kind: "qualified_type",
        required_fields: &["name", "package"],
    },
    // 函数/方法
    Contract {
        kind: "function_declaration",
        required_fields: &["name", "body"],
    },
    Contract {
        kind: "method_declaration",
        required_fields: &["receiver", "name", "body"],
    },
    // 调用/选择器/构造
    Contract {
        kind: "call_expression",
        required_fields: &["function", "arguments"],
    },
    Contract {
        kind: "selector_expression",
        required_fields: &["operand", "field"],
    },
    Contract {
        kind: "composite_literal",
        required_fields: &["type", "body"],
    },
    // 并发危险信号（detect_tier 扫描）
    Contract {
        kind: "channel_type",
        required_fields: &["value"],
    },
    Contract {
        kind: "go_statement",
        required_fields: &[],
    },
    Contract {
        kind: "send_statement",
        required_fields: &["channel", "value"],
    },
    Contract {
        kind: "select_statement",
        required_fields: &[],
    },
];

/// 覆盖 [`CONTRACTS`] 全部 kind 的 Go 源码。
const SRC: &str = r#"
package sample

import (
	"fmt"
	"sync"
)

const Version = "1.0"

var mu sync.Mutex

type Shape interface {
	Area() float64
}

type Rect struct {
	W float64
	H float64
}

func (r Rect) Area() float64 {
	return r.W * r.H
}

func describe(s Shape) string {
	return fmt.Sprintf("%v", s.Area())
}

func run() {
	r := Rect{W: 1, H: 2}
	_ = r.Area()
	ch := make(chan int)
	go func() {
		ch <- 1
	}()
	select {
	case v := <-ch:
		_ = v
	}
}
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
fn ast_contract_go_holds() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_go::language())
        .expect("加载 tree-sitter-go 语法失败");
    let tree = parser.parse(SRC, None).expect("解析测试源码失败");
    let root = tree.root_node();
    assert!(
        !root.has_error(),
        "SRC 含语法错误，契约测试的字段断言会失真——请修正 SRC"
    );

    for c in CONTRACTS {
        let node = find_first_kind(root, c.kind).unwrap_or_else(|| {
            panic!(
                "kind `{}` 未在 AST 中出现：grammar 可能重命名/移除了它，\
                 或 SRC 未覆盖它（go.rs 仍硬编码依赖该 kind）",
                c.kind
            )
        });
        for field in c.required_fields {
            assert!(
                node.child_by_field_name(field).is_some(),
                "kind `{}` 的 required field `{}` 取不到：grammar 可能改了字段名/结构，\
                 go.rs（现在或 PR-C2/C3）中 child_by_field_name(\"{}\") 将静默失效",
                c.kind,
                field,
                field
            );
        }
    }
}
