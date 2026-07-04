// Command main 是菱形顶点，依赖 left 与 right（二者再共同依赖 geom）。
package main

import (
	"example.com/diamond/left"
	"example.com/diamond/right"
)

func main() {
	// 跨包调用 left.LeftVersion / right.RightVersion（目标在各自代表文件）。
	_ = left.LeftVersion()
	_ = right.RightVersion()
}
