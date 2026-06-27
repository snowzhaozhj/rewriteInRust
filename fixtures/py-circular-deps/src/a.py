"""环成员 a：依赖 b 和 shared，与 b 构成循环。"""
from .b import b_func
from .shared import helper


def a_func() -> int:
    return b_func() + helper()
