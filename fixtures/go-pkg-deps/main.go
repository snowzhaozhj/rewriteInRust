// Command main 依赖 store 包，跨包调用代表文件内的 store.Query。
package main

import "example.com/pkgdeps/store"

func main() {
	_ = store.Query(21)
}
