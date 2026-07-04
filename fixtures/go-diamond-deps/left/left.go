// Package left 是菱形左臂，依赖 geom。
package left

import "example.com/diamond/geom"

// LeftVersion 跨包引用 geom.Version（selector，非调用 → 仅 imports 边）。
func LeftVersion() string {
	return geom.Version
}
