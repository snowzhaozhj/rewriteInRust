//go:build ignore

// 被 //go:build ignore 门控：analyze_file 仍建 File 节点，但跳过符号提取 → 孤立 File 节点。
package store

// Ignored 不应作为符号节点出现（仅保留 file:store/tagged.go）。
func Ignored() {}
