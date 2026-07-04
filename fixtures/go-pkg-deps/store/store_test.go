package store

import "testing"

// TestSave 位于 _test.go——应被 can_handle 完全排除（图中不出现任何节点，含 File）。
// 构造非空 Store 使其 `go test` 亦真实通过（fixture 保持可信）。
func TestSave(t *testing.T) {
	s := Store{Name: "x"}
	if !s.Save() {
		t.Fatal("expected true")
	}
}
