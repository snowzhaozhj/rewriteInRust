"""菱形右支：继承 Serializable。"""
from .base import Serializable


class UserModel(Serializable):
    def __init__(self, name: str):
        self.name = name

    def serialize(self) -> str:
        return self.name
