// 平台后缀文件（_windows.go）——非默认目标 linux/amd64，应被 can_handle 完全排除（无节点）。
package store

// WinOnly 仅 windows 构建，不应出现在图中。
func WinOnly() string {
	return "win"
}
