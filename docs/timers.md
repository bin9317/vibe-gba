# GBA Timers Technical Documentation

The GBA has four identical 16-bit timers (Timer 0 to Timer 3).

## I/O Registers

| Address | Name | Description |
| :--- | :--- | :--- |
| 04000100h | TM0CNT_L | Timer 0 Reload Value (W) |
| 04000102h | TM0CNT_H | Timer 0 Control (R/W) |
| 04000104h | TM1CNT_L | Timer 1 Reload Value (W) |
| 04000106h | TM1CNT_H | Timer 1 Control (R/W) |
| 04000108h | TM2CNT_L | Timer 2 Reload Value (W) |
| 0400010Ah | TM2CNT_H | Timer 2 Control (R/W) |
| 0400010Ch | TM3CNT_L | Timer 3 Reload Value (W) |
| 0400010Eh | TM3CNT_H | Timer 3 Control (R/W) |

## Timer Control (TMxCNT_H)

| Bits | Description |
| :--- | :--- |
| 0-1 | **Prescaler Selection**: <br> 0: Freq/1 (16.78 MHz) <br> 1: Freq/64 (262.144 kHz) <br> 2: Freq/256 (65.536 kHz) <br> 3: Freq/1024 (16.384 kHz) |
| 2 | **Count-up Timing (Cascade Mode)**: <br> 0: Use Prescaler <br> 1: Increment when previous timer overflows (Not applicable for Timer 0) |
| 3-5 | Not used |
| 6 | **Timer IRQ Enable**: <br> 0: Disable IRQ <br> 1: Request IRQ on overflow |
| 7 | **Timer Operating Status**: <br> 0: Stop <br> 1: Start/Operate |
| 8-15 | Not used |

## Operational Details

### 1. Counting and Overflow
- A timer increments at the selected frequency.
- When it overflows (exceeds FFFFh), it is automatically reloaded with the value in `TMxCNT_L`.
- If IRQ is enabled (bit 6), an interrupt is requested upon overflow.

### 2. Cascade Mode (Bit 2)
- When enabled, the timer increments only when the **preceding** timer overflows (e.g., Timer 1 counts when Timer 0 overflows).
- This allows for 32-bit, 48-bit, or 64-bit timers by chaining them.
- **Timer 0** ignores this bit as it has no predecessor.

### 3. Start/Stop (Bit 7)
- When changed from 0 to 1, the timer is initialized with the reload value.
- The internal counter starts ticking immediately based on the selected prescaler.

## Timing Calculation
The system clock is **16.777216 MHz**.
Ticks per second = 16,777,216 / Prescaler.
Overflow frequency = (16,777,216 / Prescaler) / (65536 - Reload Value).
