// Package utils 提供基础数值工具。链路末端，不依赖其他本地包。
package utils

// MaxValue 是导出的模块级常量（激活 Variable 节点 + 导出约定）。
const MaxValue = 100

// clampLo 是未导出的模块级变量（首字母小写 → 不导出）。
var clampLo = 0

// Clamp 将 v 限制在 [lo, hi] 区间（首字母大写 → 导出）。
func Clamp(v, lo, hi int) int {
	if v < lo {
		return lo
	}
	if v > hi {
		return hi
	}
	return v
}

// MinMax 返回排序后的两值（多返回值签名）。
func MinMax(a, b int) (int, int) {
	if a < b {
		return a, b
	}
	return b, a
}
