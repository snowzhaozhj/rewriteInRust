// Package b 回指 a，闭合环（b→a）。
package b

import "example.com/circular/a"

// Pong 调用 a 侧函数，形成 b→a 依赖，与 a→b 闭环。
func Pong() string {
	return a.Ping()
}
