// Package a 与 b 构成包级环（a→b→a）。
// 注意：真实 `go build` 禁止导入环——本 fixture 为静态图的环检测（SCC）验证而设，
// 不要求可编译。shared 被引用但不在环内。
package a

import (
	"example.com/circular/b"
	"example.com/circular/shared"
)

// Ping 调用 b 侧函数，形成 a→b 依赖。
func Ping() string {
	return b.Pong() + shared.Tag()
}
