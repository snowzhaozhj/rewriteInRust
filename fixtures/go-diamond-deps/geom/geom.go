// Package geom 是菱形底部（被 left/right 共同依赖）。
// 集中演示 interface、struct 嵌入（extends）与隐式接口实现（不连 Implements 边）。
package geom

// Version 是导出常量，供 left/right 引用（触发跨包 imports 边，无 calls 边）。
const Version = "1.0"

// Shape 是导出 interface（→ Interface 节点）。
type Shape interface {
	Area() float64
}

// Base 是导出的公共基 struct（→ Class 节点）。
type Base struct {
	Name string
}

// Circle 同包嵌入 Base（struct 嵌入 → extends 边），并隐式实现 Shape。
type Circle struct {
	Base
	R float64
}

// Area 使 Circle 隐式满足 Shape——但不应产生 Implements 边（D-M4-02）。
func (c Circle) Area() float64 {
	return 3.14 * c.R * c.R
}
