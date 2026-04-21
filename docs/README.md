# Documentation Index

当前保留的文档分成三类：

## 入门与日常使用

- [system_design.md](system_design.md)
  系统架构、模块职责、稳定命令入口。
- [debugging_guide.md](debugging_guide.md)
  日常调试流程、CLI 命令、前端即时存档和截图工具。

## 回归测试

- [gba-tests.md](gba-tests.md)
  `gba-tests` / `mgba-suite` 的实际运行方式和结果解释。
- [gba-tests-results.md](gba-tests-results.md)
  当前已经记录的兼容性结果。

## 设计与专项调试

- [timing_system_design.md](timing_system_design.md)
  时序系统的设计目标和约束。
- [timing_ut_debugging.md](timing_ut_debugging.md)
  `mgba-suite` 时序失败时的单测迁移与定位流程。
- [timers.md](timers.md)
  GBA timer 的寄存器与行为摘要。

已删除的内容主要是阶段性复盘和一次性排障记录。这类材料如果没有继续维护，信息价值会快速衰减，并且容易和当前实现脱节。
