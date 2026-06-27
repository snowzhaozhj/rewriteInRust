"""环成员 b：依赖 a 和 shared，与 a 构成循环。"""
from .a import a_func
from .shared import helper


def b_func() -> int:
    return helper()


def b_other() -> int:
    return a_func()
