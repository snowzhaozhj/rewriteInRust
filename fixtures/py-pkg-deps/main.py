"""顶层模块：通过包入口导入 re-export 的符号。"""
from pkg import Worker


def go() -> int:
    w = Worker()
    return w.run()
