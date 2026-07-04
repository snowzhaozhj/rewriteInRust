// Package store 的一部分（多文件同包 → decompose 应凝聚到同一 DecompUnit）。
package store

// Store 是导出的存储句柄（struct → Class）。
type Store struct {
	Name string
}

// Save 是导出方法。位于非代表文件（store.go 字典序晚于 query.go），
// 故跨包 store.Save 调用会被漏边（已知精度限制）——本 fixture 只跨包调用代表文件内的 Query。
func (s Store) Save() bool {
	return s.Name != ""
}
