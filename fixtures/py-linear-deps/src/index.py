"""顶层入口：依赖 service 和 utils。"""
from .service import NumberService
from .utils import Range


def main():
    svc = NumberService(Range(0, 10))
    print(svc.normalize(5))
