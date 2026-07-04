// Command main 是线性链路顶端，依赖 service。
package main

import "example.com/linear/service"

func main() {
	// 跨包调用 service.Run（目标位于 service 包代表文件）。
	_ = service.Run(10)
}
