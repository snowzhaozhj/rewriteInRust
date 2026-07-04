package store

import "testing"

// TestSave 位于 _test.go——应被 can_handle 完全排除（图中不出现任何节点，含 File）。
func TestSave(t *testing.T) {
	var s Store
	if !s.Save() {
		t.Fatal("empty")
	}
}
