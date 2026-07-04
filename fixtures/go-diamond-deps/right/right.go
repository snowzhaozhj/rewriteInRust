// Package right 是菱形右臂，依赖 geom。
package right

import "example.com/diamond/geom"

// RightVersion 跨包引用 geom.Version（selector，非调用 → 仅 imports 边）。
func RightVersion() string {
	return geom.Version
}
