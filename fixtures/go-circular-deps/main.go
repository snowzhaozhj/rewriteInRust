// Command main 依赖 a 与 shared（不引入新环）。
package main

import (
	"example.com/circular/a"
	"example.com/circular/shared"
)

func main() {
	_ = a.Ping()
	_ = shared.Tag()
}
