"""中间层：依赖 utils，提供业务服务。"""
from .utils import clamp, fetch_data, Range


class NumberService:
    def __init__(self, value_range: Range):
        self.value_range = value_range

    def normalize(self, value: int) -> int:
        return clamp(value, self.value_range.low, self.value_range.high)

    async def load(self, url: str) -> list:
        data = await fetch_data(url)
        return [self.normalize(v) for v in data]
