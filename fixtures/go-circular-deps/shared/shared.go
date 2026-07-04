// Package shared 是环外叶子（被 a 引用，但不参与任何环）。
package shared

// Tag 返回固定标记。
func Tag() string {
	return "shared"
}
