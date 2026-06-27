"""纯类型定义模块（叶子，不依赖任何模块）。仅被 base 的 TYPE_CHECKING 块引用。"""

__all__ = ["Config"]


class Config:
    def __init__(self, verbose: bool = False):
        self.verbose = verbose
