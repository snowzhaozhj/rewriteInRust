// Package store 的另一文件（同包，与 store.go 凝聚）。
// query.go 字典序第一（非 _test.go）→ 作为包「代表文件」承载跨包解析。
package store

// Query 是导出函数，位于包代表文件，可被跨包 store.Query 精确解析。
func Query(id int) int {
	return id * 2
}
