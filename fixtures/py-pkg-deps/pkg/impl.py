"""具体实现：继承 Base，含全局常量与静态方法。"""
from .base import Base

GLOBAL_CONST = 42


class Worker(Base):
    def run(self) -> int:
        return GLOBAL_CONST

    @staticmethod
    def helper() -> None:
        pass
