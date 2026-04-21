# Debugging Guide

这份文档只记录仓库里当前仍然保留、可复用的调试入口。

## 先选最小工具

- 先确认画面是否在变化：用 `snapshot_frames`
- 先确认 CPU 是否还在跑：用 `cli_debugger`
- 先确认是不是某个即时场景：用 `frontend` 的 savestate
- 先确认是不是底层回归：跑相关 Rust 回归测试和 `run_suite`

不要一上来加 ROM 专用 hack。先证明是哪个子系统错了。

## Frontend

启动：

```bash
cargo run --release -p frontend -- <rom>
```

默认键位：

- `Z` A
- `X` B
- `Backspace` Select
- `Enter` Start
- 方向键 D-pad
- `A` L
- `S` R
- `F5` 保存状态
- `F8` 读取状态

多槽位即时存档：

- `Cmd/Ctrl + 0..9` 选择槽位
- `Cmd/Ctrl + S` 保存到当前槽位
- `Cmd/Ctrl + L` 从当前槽位加载

文件命名：

- 槽位 `0`：`<rom>.state`
- 槽位 `1`：`<rom>.state.1`
- 槽位 `2`：`<rom>.state.2`

这一步的意义是把“难复现问题”压缩成一个可重复加载的状态。

## CLI 调试器

启动：

```bash
cargo run -p cli_debugger --bin cli_debugger -- <rom>
```

如果你只想跳过 BIOS：

```bash
cargo run -p cli_debugger --bin cli_debugger -- <rom> --skip-bios
```

`--skip-bios` 是调试专用路径。正常 frontend 启动要求仓库根目录存在 `gba_bios.bin`，缺失时会直接退出。

常用命令：

- `step` / `s [n]`：执行 `n` 条 CPU 指令
- `frame` / `f [n]`：推进 `n` 帧
- `regs`：打印寄存器
- `dis`：打印当前 PC 和当前指令
- `ppu`：打印 PPU 寄存器摘要
- `dma`：打印 DMA 通道摘要
- `run_until <pc_hex> [max_steps]`
- `trace_pc <pc_hex>`：添加 PC 命中点
- `trace_run [max_steps] [max_hits]`
- `watch <addr_hex> [1|2|4]`：在 trace hit 时打印内存
- `trace_status`
- `dump_mem <addr_hex>`：读取 16 字节
- `poke <addr_hex> <value_hex> [1|2|4]`
- `key <mask_hex>`：直接设置 GBA `KEYINPUT`
- `snapshot <path.png|path.bmp>`

适用场景：

- ROM 卡在固定 PC
- 怀疑某个标志位没有变化
- 需要确认某次内存写是否真的发生
- 需要在不打开窗口的情况下导出当前帧

## `snapshot_frames`

启动：

```bash
cargo run --release -p cli_debugger --bin snapshot_frames -- \
  <rom> \
  --output-dir artifacts/snapshots \
  --image-format png \
  --snapshot-every-frames 60 \
  --max-frames 600
```

常用参数：

- `--load-state <path>`
- `--bios gba_bios.bin`
- `--skip-bios`
- `--snapshot-every-frames <n>`
- `--max-frames <n>`
- `--stall-threshold-pixels <n>`
- `--stall-limit <n>`
- `--capture-initial-frame`
- `--press-a-at-frame <frame> [hold_frames]`
- `--press-start-at-frame <frame> [hold_frames]`
- `--press-mask-at-frame <frame> <mask_hex> [hold_frames]`

这个工具会输出：

- 周期性截图
- 相邻截图的像素差
- 连续静止帧计数

如果你加载的是 savestate，并且只想把 state 里已经保存好的 framebuffer 导出来，不想先推进一帧：

```bash
cargo run --release -p cli_debugger --bin snapshot_frames -- \
  <rom> \
  --load-state <rom.state> \
  --output-dir artifacts/state-frame \
  --capture-initial-frame \
  --max-frames 0
```

适用场景：

- 黑屏或静止画面
- 某个动画后才出现的错误
- 修复前后做截图对比

从即时存档开始是最常见的用法：

```bash
cargo run --release -p cli_debugger --bin snapshot_frames -- \
  <rom> \
  --load-state <rom.state> \
  --output-dir artifacts/snapshots \
  --snapshot-every-frames 1 \
  --max-frames 30
```

## `run_suite`

运行 `mgba-suite`：

```bash
cargo run --release -p cli_debugger --bin run_suite -- <path-to-mgba-suite.gba>
```

可选参数：

- 第一个参数：suite ROM 路径
- 第二个参数：BIOS 路径，默认 `gba_bios.bin`
- 第三个参数：单个 suite 的超时帧数

默认输出只保留失败项上下文。若要看完整日志：

```bash
SHOW_ALL=1 cargo run --release -p cli_debugger --bin run_suite -- <path-to-mgba-suite.gba>
```

适用场景：

- 验证 CPU / 总线 / DMA / timer 的系统性回归
- 修完底层 bug 后做批量复验

## `trace_boot`

当 BIOS 启动阶段就卡住时：

```bash
cargo run -p cli_debugger --bin trace_boot -- <rom>
```

它会打印：

- IRQ 入口
- BIOS 等待循环
- `DISPSTAT` / `IE` / `IF` / `IME` 变化
- 是否真正跳入 `0x08000000` 的 ROM 区间

适合定位“连游戏主程序都没进去”的问题。

## 推荐流程

1. 在 `frontend` 复现，并保存一个最小场景的 savestate。
2. 用 `snapshot_frames --load-state` 判断症状是“停住”“部分错误”还是“持续变化但不对”。
3. 用 `cli_debugger` 看 PC、寄存器、关键地址和输入。
4. 若怀疑是底层回归，跑相关 Rust 回归测试和 `run_suite`。
5. 修复后优先补正式测试，不要继续把一次性探针留在仓库里。
