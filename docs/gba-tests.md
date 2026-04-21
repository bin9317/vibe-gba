# External Test Suites

仓库当前用两类外部 ROM 回归：

- `third_party/gba-tests`
- `<path-to-mgba-suite.gba>` 对应的 `mgba-suite`

它们的用途不同：

- `gba-tests` 更适合看单个兼容性 ROM 的最终屏幕结果
- `mgba-suite` 更适合自动批量跑并从 SRAM 日志中抽取失败项

## 1. `gba-tests`

目录：

- `third_party/gba-tests/arm`
- `third_party/gba-tests/thumb`
- `third_party/gba-tests/memory`
- `third_party/gba-tests/bios`
- `third_party/gba-tests/ppu`
- `third_party/gba-tests/save`

### 手动运行

```bash
cargo run --release -p frontend -- third_party/gba-tests/arm/arm.gba
```

### 无窗口截图运行

```bash
cargo run --release -p cli_debugger --bin snapshot_frames -- \
  third_party/gba-tests/arm/arm.gba \
  --output-dir artifacts/gba-tests/arm \
  --image-format png \
  --snapshot-every-frames 120 \
  --max-frames 720
```

### 结果解释

`gba-tests` 通常把结果直接画到屏幕上：

- 通过：显示成功画面
- 失败：显示第一个失败测试编号

因此最实用的流程是：

1. 运行 ROM
2. 保留最后稳定截图
3. 记录 ROM 名、通过/失败、首个失败编号
4. 回到对应的 `.asm` 源文件定位测试含义

仓库里已经有一个批量脚本：

```bash
scripts/run_gba_tests.sh [output_dir]
```

默认会跑：

- `arm.gba`
- `thumb.gba`
- `memory.gba`
- `bios.gba`

## 2. `mgba-suite`

运行：

```bash
cargo run --release -p cli_debugger --bin run_suite -- <path-to-mgba-suite.gba>
```

该工具会自动：

- 等待主菜单
- 逐个进入可自动运行的 suite
- 等待 suite 执行完成
- 返回菜单后进入下一项
- 从 SRAM 文本日志里提取 `FAIL`

如果需要完整日志而不是只看失败：

```bash
SHOW_ALL=1 cargo run --release -p cli_debugger --bin run_suite -- <path-to-mgba-suite.gba>
```

### 什么时候用

- CPU、总线、DMA、Timer、I/O 语义改动后
- 某个 bug 看起来不像单个商业 ROM 特例时
- 时序回归需要批量确认时

## 3. 失败后怎么处理

1. 先确认失败来自哪一个 ROM 或哪一类 suite。
2. 如果是 `gba-tests`，记录屏幕上的失败编号并回到对应源文件。
3. 如果是 `mgba-suite`，先看 `FAIL` 上下文，再定位到相关测试源码。
4. 把最小失败案例迁移到 Rust 单元/集成测试，避免每次都靠整 ROM 复跑。
5. 修复后重新跑相关 ROM，再跑对应的 Rust 回归测试。

## 4. 当前结果

已记录的结果见：

- [gba-tests-results.md](gba-tests-results.md)
