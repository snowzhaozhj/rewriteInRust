"""线性依赖叶节点：纯工具函数 + 数据类。"""
from typing import List

__all__ = ["clamp", "fetch_data", "Range"]

DEFAULT_MIN = 0


def clamp(value: int, low: int, high: int) -> int:
    return max(low, min(value, high))


async def fetch_data(url: str) -> List[int]:
    return [1, 2, 3]


class Range:
    def __init__(self, low: int, high: int):
        self.low = low
        self.high = high
