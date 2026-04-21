# System Architecture

`vibe-gba` 是一个用 Rust 实现的 Game Boy Advance 模拟器工作区，当前仓库分成三个主要 crate：

- `gba_core`：模拟器内核，包含 CPU、总线、PPU、DMA、Timer、存档介质与状态序列化。
- `frontend`：基于 `winit` + `pixels` 的桌面前端，负责窗口显示、实时输入、即时存档读写。
- `cli_debugger`：无窗口调试工具集，包含交互式 CLI、批量截图工具和外部测试套件运行器。

## Entrypoints

- 运行桌面前端：

```bash
cargo run --release -p frontend -- tests/roms/armwrestler.gba
```

- 启动交互式 CLI 调试器：

```bash
cargo run -p cli_debugger --bin cli_debugger -- tests/roms/armwrestler.gba
```

- 周期性截图并检测画面是否停滞：

```bash
cargo run --release -p cli_debugger --bin snapshot_frames -- \
  tests/roms/armwrestler.gba \
  --output-dir artifacts/snapshots \
  --image-format png \
  --snapshot-every-frames 60 \
  --max-frames 600
```

- 从 savestate 当前 framebuffer 直接导出首帧，不先推进模拟：

```bash
cargo run --release -p cli_debugger --bin snapshot_frames -- \
  tests/roms/armwrestler.gba \
  --load-state tests/roms/armwrestler.gba.state \
  --output-dir artifacts/state-frame \
  --capture-initial-frame \
  --max-frames 0
```

- 跑 `mgba-suite` 自动回归：

```bash
cargo run --release -p cli_debugger --bin run_suite -- <path-to-mgba-suite.gba>
```

- 跑常用 Rust 回归测试：

```bash
cargo test -q -p gba_core --test mgba_timing_loadstore_test
cargo test -q -p gba_core --test bus_test
cargo test -q -p gba_core --test ppu_test
```

## Architecture Overview

### 1. 执行模型

`gba_core::Gba` 只包含两部分状态：

- `cpu::Cpu`
- `bus::Bus`

`Gba::step()` 的职责很小：

- 先检查 `IE/IF/IME`，必要时进入 IRQ
- 如果 CPU 未 halt，则执行一条 CPU 指令
- 如果 CPU halt，则只推进总线时钟

这意味着大部分硬件推进都挂在 `Bus::clock()` 上，而不是散落在 CPU 指令实现里。

### 2. 总线与硬件同步

`gba_core/src/bus.rs` 是内核的中心协调层，负责：

- BIOS、WRAM、VRAM、OAM、ROM、SRAM/Flash、EEPROM 映射
- waitstate / prefetch / open-bus 相关行为
- 在 `clock()` 中同步推进 PPU、Timer、DMA 和中断请求
- 维护 `KEYINPUT`、`WAITCNT`、`POSTFLG` 等关键 I/O 状态

仓库当前的设计重点是“按访问推进硬件时间”，所以 CPU 行为和 PPU/Timer/DMA 的可见状态靠同一个时钟源保持同步。

### 3. 图形与前端

- `gba_core/src/ppu/` 负责寄存器、扫描线推进与 framebuffer 生成
- `frontend/src/main.rs` 每帧调用 `gba.step()` 直到达到 `GBA_CYCLES_PER_FRAME`
- 生成好的 RGBA framebuffer 直接交给 `pixels` 渲染

前端额外提供：

- 键位映射
- 多槽位即时存档
- SRAM / EEPROM 存档文件加载与落盘

### 4. 状态与存档

- 即时存档由 `gba_core/src/state.rs` 负责序列化，文件名是 `<rom>.state` 或 `<rom>.state.<slot>`
- 电池存档由 `frontend/src/save.rs` 管理
- SRAM 保存到 `<rom>.sram.sav`
- EEPROM 保存到 `<rom>.eeprom.sav`
- 旧格式 `<rom>.sav` 仍可读取并自动判别

## Usage

### Frontend

默认键位：

- `Z` = A
- `X` = B
- `Backspace` = Select
- `Enter` = Start
- 方向键 = D-pad
- `A` = L
- `S` = R
- `F5` = 保存即时存档
- `F8` = 读取即时存档
- `Cmd/Ctrl + 0..9` = 选择存档槽位
- `Cmd/Ctrl + S` = 保存到当前槽位
- `Cmd/Ctrl + L` = 从当前槽位加载

可选配置文件：

- `~/.config/vibe-gba/config.json`

### CLI 调试器

`cli_debugger` 适合确认 CPU 是否卡死、某个 PC 是否命中、某个内存地址是否被写入。

常用命令：

- `step` / `s [n]`
- `frame` / `f [n]`
- `run_until <pc_hex> [max_steps]`
- `trace_pc <pc_hex>`
- `trace_run [max_steps] [max_hits]`
- `watch <addr_hex> [1|2|4]`
- `trace_status`
- `regs`
- `dis`
- `dump_mem <addr_hex>`
- `poke <addr_hex> <value_hex> [1|2|4]`
- `key <mask_hex>`
- `snapshot <path.png|path.bmp>`

### 其他调试命令

- `snapshot_frames`：批量截图、注入按键、检测画面停滞
- `snapshot_frames --capture-initial-frame`：直接导出 savestate 内已有 framebuffer
- `run_suite`：自动驱动 `mgba-suite` 并从 SRAM 日志中提取 `FAIL`
- `trace_boot`：追踪 BIOS 到 ROM 启动阶段的状态变化，适合定位早期卡死

## Recommended Debugging Flow

1. 先在 `frontend` 复现问题并存一个即时存档。
2. 用 `snapshot_frames --load-state` 判断画面是在变化、半错还是完全停滞。
3. 再用 `cli_debugger` 看 PC、寄存器、关键内存和输入状态。
4. 如果像是底层硬件回归，跑相关 Rust 回归测试和 `run_suite`。
5. 修复后优先补 Rust 测试，而不是继续堆临时 probe。

## Related Docs

- [README.md](README.md)
- [debugging_guide.md](debugging_guide.md)
- [gba-tests.md](gba-tests.md)
- [gba-tests-results.md](gba-tests-results.md)
- [timing_system_design.md](timing_system_design.md)
- [timing_ut_debugging.md](timing_ut_debugging.md)
- [timers.md](timers.md)
