"""独立叶节点：被 a 和 b 依赖，但不依赖任何模块，不应出现在环中。"""

__all__ = ["helper"]


def helper() -> int:
    return 1
