// Package service 依赖 utils，向上被 main 依赖（线性链路中段）。
package service

import "example.com/linear/utils"

// Scaler 是导出的缩放器（struct → Class 节点）。
type Scaler struct {
	Factor int
}

// Scale 是 Scaler 的方法（值 receiver，限定名 Scaler.Scale）。
// 跨包调用 utils.Clamp（目标位于 utils 包代表文件）。
func (s Scaler) Scale(v int) int {
	return utils.Clamp(v*s.Factor, 0, utils.MaxValue)
}

// Run 同包构造 Scaler 并调用其方法（同包构造 → Constructor 边 + 方法绑定）。
func Run(v int) int {
	s := Scaler{Factor: 2}
	return s.Scale(v)
}
