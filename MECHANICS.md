# SteamlessController for CachyOS — mechanics sheet

## Where the values come from

Two sources, which agree:

1. **The source code** (`~/Downloads/SteamlessController-1.25.tar.gz`, sha256
   `c069465d65af…`). "Source" gives the **file and line** in its `src/` folder.
2. **The compiled program**, `SteamlessController.exe` 1.25 (x64, sha256
   `2e43ef47cd72…`), taken out of `~/Downloads/SteamlessController-Setup.exe`
   (Inno Setup 6.7, sha256 `3de8ab95cd26…`) without running it: the installer's
   single LZMA2 file chunk was unpacked and Inno's x86 call/jump filter undone
   (64 KiB blocks), then the program was decompiled. The table below gives the
   **function address** each value was read from (evidence ids in the
   Craft Anything project).

Every value checked in the program matched the source. Two rules were finer in the
program than in the Linux build and were changed to match: key repeat interval
rounding and when the event log rotates; and trackpad scrolling now applies the
program's wheel-lines correction (marked **changed** / **added** below). Values marked *estimated* or *Linux* are not in the original;
they are my choices for the Linux port.

### Checked in the compiled program

| What | Value in the program | Function | Linux build |
|---|---|---|---|
| Lizard off | feature `[01 81]`, then `[01 87 03 30 00 00]`, then `[01 87 06 08 00 00 07 00 00]`, each 64 bytes | `0x14004ab40` (DisableLizardMode; sends via `0x14004d5e0` → HidD_SetFeature) | `protocol.rs` ✓ |
| Lizard on | `[01 85]`, then `[01 8E]` | `0x14004ada0` (EnableLizardMode) | `protocol.rs` ✓ |
| Keepalive | elapsed ≥ 2 000 000 000 ns (`cmp rcx,0x77359400` at `0x14001a727`) | `0x14001a500` (ReadLoop) | `KEEPALIVE` 2 s ✓ |
| Stall warning | elapsed > 4 s (`0xee6b2801` ns at `0x14001ac49`) | `0x14001a500` | `STALL` 4 s ✓ |
| Power-off grace | still reporting after > 2 999 999 999 ns (`0x14001b30b`) | `0x14001a500` | `POWER_OFF_GRACE` 3 s ✓ |
| Battery / state reports | ids 0x43; 0x42 / 0x45 | `0x14001a500` (`0x14001b0b4`, `0x14001af28`) | ✓ |
| Pad press | contact area ≥ 1800 (`0x708`, `0x14001b3f3`) | `0x14001a500` | `PAD_PRESS_AREA` ✓ |
| Tap | ≤ 50 frames and travel ≤ 3000 → 12-frame pulse (`0x14001b5d6`–`0x14001b5e7`) | `0x14001a500` | `TAP_*` ✓ |
| Trigger edge | > 0x2000 (`0x14001cde8`) | `0x14001a500` | `controller.rs` ✓ |
| Rumble model | deadzone 6, /249; left = low + high·0.30; right = low·0.78 + high·0.88; punch ·0.55 / ·0.38, threshold 0.08; boost +punch·0.70 + low·0.08 / +punch·0.62 + high·0.10; boost 55 ms, 80 ms if large > 179; trigger buzz floor 0x5000 | `0x14004bdc0` (SetRumble; doubles at `0x140076a20`…`0x140076a30`) | `mapping.rs` ✓ |
| Speed curve | `min + round(v^0.68 · (0xFFFF − min))`, min 0x1200 base / 0x2200 boost | `0x14004c660` (UnitToHapticSpeed; exponent at `0x140076a00`) | `unit_to_speed` ✓ |
| Steam watcher | sleeps 100 ms × 20 = 2 s; more activity at once; the same lower state 3 polls in a row (`2 < count`) | `0x140025000` (PollLoop) | `observe_steam` ✓ |
| Steam check / liveness timers | 5000 ms, 2000 ms (SetTimer) | `0x14000b90f`, `0x14000edfa` | ✓ |
| Stick settings load | mode 0/1/2 (else 0); inner default 8, capped at 95; outer default 100, clamped to inner + 5 … 100 | `0x1400043f0` (stick profile load) | `clamp_stick_range` ✓ |
| Trigger settings load | mode 0/1 (else 0); press default 95, clamped 20 … 100 (limits at `0x140056e58`/`0x140056e5c`); release default 80, clamped 5 (`0x140056e60`) … press − 3 (5 when press < 4); click feel default 2 (Press and release), ≥ 3 → 2 | `0x140004960` (trigger profile load) | `clamp_trigger_press/release` ✓ |
| Trackpad settings load | mode < 6, scroll direction < 2; scroll speed default 100, clamped 10 … 400 (`0x14000abd9`–`0x14000abf1`) | `0x14000a6e0` (trackpad load); saved by `0x140009ba0` | `sanitize_profile` ✓ |
| Key repeat delay | SPI_GETKEYBOARDDELAY, setting > 3 or failure → 1; delay = (setting + 1) × 250 ms | `0x140021b40` (KeyRepeatDelay) | fallback 500 ms ✓ |
| Key repeat interval | SPI_GETKEYBOARDSPEED, > 31 or failure → 31; interval = (long)(1000 / (2.5 + 27.5 × setting / 31) + 0.5) ms, i.e. **whole milliseconds, rounded** | `0x140021b90` (KeyRepeatInterval; doubles at `0x14005aa50`, `0x14005b9a0`…`0x14005b9b0`, `0x14005ad70`) | **changed**: `key_repeat()` now rounds 1000 / rate to whole ms (30/s → 33 ms) instead of an exact fraction |
| Trackpad pointer | per frame while touched: f = speed/100 × 0.01125 (`0x14005ad64`); x += dx·f, y −= dy·f, whole pixels out (truncated), fraction kept | `0x140020dd0` (TrackpadInput update) | `TrackpadInput::update` ✓ |
| Trackpad scroll | Natural (setting 0) negates dx, dy; f = speed/100 × (3 / wheel lines) × 0.02 (`0x14005ad68`); vertical = dy·f, horizontal = dx·f, truncated, fraction kept | `0x140020dd0` | **changed**: the wheel-lines factor was missing |
| Wheel lines | SPI_GETWHEELSCROLLLINES, re-read at most once a second; 0 or failure → 3; page scrolling (−1) → 20 (`0x14005c508`) | `0x1400246e0` | **added** `wheel_correction()`: KDE `WheelScrollLines` in `~/.config/kdeglobals` [KDE], same rules *Linux* |
| Jump log | a touched-frame move ≥ 6000 units (`0x14005adb0`) is logged as TRACKPAD, at most every 3 s | `0x140020dd0` | log only, not ported |
| Directional pad | touch starting inside radius 12000 (`0x14005adb8`) stays neutral until release; angle = atan2(y, x)·180/π in 0…360; step 45° (8-way) or 90° (4-way); keep the sector while within step·0.5 + 8° (`0x14005ad88`), else sector = floor((angle + step/2)/step) mod n; sectors → d-pad bits right, up+right, up, up+left, left, down+left, down, down+right | `0x1400212c0` (dpad sector) | `resolve_sector`, `sector_dirs` ✓ |
| Stick shaping | mode Default: radial, inner 0xA3D (2621 = 8 %), outer 0x7FFF; Circle: radial with inner = inner% × 0x7FFF / 100 (inner% ≤ 95), outer = clamp(outer%, inner% + 5, 100) × 0x7FFF / 100; Square: per axis, \|v\| ≤ inner → 0, t = (\|v\| − inner)/(outer − inner), t ≥ 1 → +32767 / −32768, else ±round(t × 32767) | `0x14001f9b0` (stick shaping, 32767.0 at `0x14005ac60`) | `apply_stick`, `stick_square` ✓ |
| Radial deadzone | inner < 1 and outer ≥ 0x7FFF → unchanged; inner clamped 0 … 0x7FFE, outer inner + 1 … 0x7FFF; m = hypot(x, y); m ≤ inner → (0, 0); else each axis = round((m − inner)/(outer − inner) × 32767 / m × v), clamped −32768 … 32767 | `0x14001fbe0` | `stick_circle` ✓ |
| Control decision | want = Manual: the switch; Off while Steam: Steam state = none; Off only in game: Steam state ≠ in game; Off unless profile: a profile is active. Not wanted while on: the two Steam modes release at once, the profile mode starts a 5000 ms timer (id 5) first; wanted while off: turn on, waiting up to 250 ms for each slot's first state report | `0x14000b7e0` (control decision, logs `CONTROL: want=…`) | `want_control_now`, `evaluate_control` ✓ (Linux turns a slot on at its first state report, with no 250 ms limit) |
| Event log | line `%04u-%02u-%02u %02u:%02u:%02u.%03u` + two spaces + message, flushed; **after** writing, if ftell > 0x80000 (512 KiB) the file is rotated to `events.old.log` (MoveFileEx replace) | `0x1400276b0` (Write), `0x1400274e0` (Rotate) | **changed**: `eventlog::write` now checks the size after writing the line, as the original does, instead of before |
| Trackpad movement ticks | float limits in `.rdata`: deadzone 45.0 (`0x14005aa68`), max area 900.0 (`0x14005aa6c`), max step 4000.0 (`0x14005aa70`), tick distance 6500.0 (`0x14005aa74`). Only when the pad wants distance (mode ≠ 4 directional pad, ≠ 5 single button), touched this and last frame, not pressed, click latch idle, touch grace over. step = hypot(dx, dy) from the stored position; post-release grace > 0, area > 900 or step > 4000 → just store the position; step < 45 → idle + 1, and only at 30 idle frames is the sum cleared and the position stored (smaller moves keep measuring from the old point); else idle = 0, sum += step, sum ≥ 6500 → tick: sent → sum = 0, not sent → sum = 6500 (retried next frame) | `0x14001a500` (ReadLoop, right pad `0x14001bf85`, left pad `0x14001c088`) | `PadHaptics::update` ✓ |
| Device search | for each PID in {0x1302, 0x1303, 0x1304, 0x1305} (array `0x13031302`, `0x13051304`): enumerate HID with VID 0x28DE, usage page 0xFF00, usage 1 | `0x14004aff0` (enumerate, helper `0x14004cc40`) | `hid.rs` ✓ |
| Power off | stop rumble (SetRumble 0, 0), then feature report 64 bytes `[01 9F 04 'o' 'f' 'f' '!']` (`'off!'` at `0x1400769a0`) | `0x14004b2d0` (PowerOff) | `controller.rs` Steam + Y ✓ |
| Click haptics | press confirmed after ≥ 2 frames (`0x14001bcc2`), release after ≥ 4 (`0x14001bcff`); on release: post-release grace 45 frames (`0x14001bd1a` right, `0x14001bdb2` left); both reset the tick position and sum; touch starts a 12-frame grace (`0x14001be1a`, `0x14001be49`) | `0x14001a500` (ReadLoop) | `PadHaptics::update` ✓ |
| Bindable inputs | the binding loop runs over 18 inputs (`cmp r15,0x12` at `0x14001cbe5`) | `0x14001a500` | `BINDABLE` ✓ |
| Trigger click buzz | per side: fewer than 40 ms (`≤ 0x26259ff` ns) since the last → ignored; else store the time, buzz until now + 60 ms (`0x3938700` ns), and **wake the rumble loop at once** (condition variable) | `0x14004b400` (QueueTriggerClick) | `queue_trigger_click` ✓; **changed**: the buzz is now sent the same frame instead of on the next 40 ms rumble turn |
| Rumble loop | after each pass, next wake = now + 40 ms (`0x2625a00` ns) | `0x14004b510` (RumbleLoop) | `RUMBLE_PERIOD` ✓ |
| Trigger to pad | Xbox 360: trigger = clamp(raw >> 7, 0, 255) (`0x140020267`, `0x1400202a0`); PlayStation the same (`0x140020534`, `0x140020554`) | `0x1400201a3` (Xbox translate), `0x1400203e8` (PlayStation translate) | `controller.rs` ✓ |
| Settings window | created 760 × 668, scaled by the screen's DPI / 96 (MulDiv); minimum size 560 × 440, also DPI-scaled (WM_GETMINMAXINFO) | `0x1400439e0` (`0x140043da7`), `0x140040b10` (`0x140040c02`) | `main.rs` ✓ (KDE scales the window itself) |
| Pad press debounce | raw press = contact area ≥ 1800 **or** the firmware click bit (byte 4 bit 0x40 right, byte 5 bit 0x04 left); 2 frames in a row to press (`0x14001b43d`) and 2 to release (`0x14001b450`) | `0x14001a500` (ReadLoop) | `PressState::update` ✓ |
| Settings load defaults | read from the registry only when a settings key exists (fresh installs keep the profile defaults: left pad scroll, right pad pointer with left click, upper paddles left click, lower paddles none). A missing value then becomes: back buttons `0xE` = None (`0x14000d038`); Rumble 100, capped at 100 (`ebx = 0x64` at `0x14000d277`, `cmova` at `0x14000d5e3`); ShowNotifications 1, any non-zero = on (`0x14000d6ac`) | `0x14000ced0` (settings load); save `0x140011720` | `config.rs` ✓ (Linux always writes the full file, so the missing-value case does not arise) |
| Trackpad load defaults | missing Mode → 0 (None), kept only if < 6; Click and Touch → `0xE` None; Up/Down/Left/Right → d-pad 8/9/10/11; ScrollDir and Diagonals → 0 (Natural, eight-way), kept only if < 2; ScrollSpeed 0 or missing → 100, else 10 … 400 | `0x14000a6e0` | `PadSettings::default` ✓ |
| Keepalive command | `[01 81]`, then (only if that worked) `[01 87 06 08 00 00 07 00 00]` (`0x7000008068701`), both 64-byte feature reports; returns the write error code on failure | `0x14004ba20` (Keepalive) | `keepalive` ✓ |
| Keepalive failures | every 2 s; on failure the try count goes up. The first failure is logged with the error, and the state/other report counters restart. Further failures are logged only at tries 3, 15 and 60 (`cmp eax,0x3c` + bit mask `0x1000000000008008` at `0x14001a8f8`), with the state and other reports seen since the first failure and the age of the last state report. The next success logs `recovered after N failed tries` and resets the count (`0x14001ab31`–`0x14001ab4c`). Nothing else is done: the keepalive simply goes out again 2 s later | `0x14001a500` (ReadLoop, `0x14001a737`) | **changed**: Linux logged only the first failure and a bare "recovered"; it now counts and logs like the original (`KeepaliveWatch`, `keepalive_report_due`) |
| Tick sending | per pad, fewer than 50 000 000 ns since the last tick → not sent; else store the time, then report `0x82 [side, 1, 0]` (side 0 left, 1 right) and return whether the write worked | `0x14004c2c0` (TickTrackpadMovement), `0x14004bc10` (haptic command) | **changed**: `Slot::pad_tick` now sends inside the check and returns whether the write worked, so a failed write keeps the 6500 and retries like the original (before, the sum was cleared even if the write failed) |

### Not checked in the compiled program

71 of the 73 values in `mechanics.json` carry a program address. The two that
don't (`profile match on linux`, `steam game detection on linux`) are Linux
replacements with no counterpart in the program. Parts that were not translated:

* **Windows-only parts, replaced on Linux**: ViGEmBus (now uinput), SendInput
  (uinput mouse/keyboard), the registry (a JSON file), Start with Windows (XDG
  autostart), the Windows tray icon (StatusNotifierItem), and choosing a profile
  by the **foreground window**. Wayland has no foreground-window query, so Linux
  matches the newest running program instead.
* **Drawing the window**: the original is plain Win32 drawing and Linux uses
  egui, so only sizes are taken (see the table above), not pixels.
* **PlayStation motion (gyro)**: not built, so not looked for.

Whether games see the PlayStation-style pad as a DS4, and how well profile
matching by program works, needs a controller in hand. Decompiling more won't
answer either.

## Device and protocol (src/steam/SteamController.h / .cpp)

| What | Value | Source |
|---|---|---|
| Vendor id | 0x28DE | `SteamController.h:12 VALVE_VID` |
| Product ids | 0x1302 wired, 0x1303 Bluetooth, 0x1304 puck, 0x1305 Nereid | `SteamController.h:13-16 SC2026_*_PID` |
| Game-input collection | usage page 0xFF00, usage 0x0001 | `SteamController.h:25-26` |
| Dock collection (no input) | usage page 0xFF00, usage 0x0002 | `SteamController.h:29 DOCK_USAGE` |
| State report ids | 0x42 (USB), 0x45 (BLE) — same layout | `SteamController.h:38-39` |
| Battery report | 0x43, payload `[chargeState, percent]` | `ControllerManager::ReadLoop` |
| Feature command buffer | 64 bytes: `[0x01, cmd, len, payload…]` | `BuildCmd` (descriptor on this PC: feature 0x01 = 63 bytes + id) |
| Lizard off | cmd 0x81 CLEAR_DIGITAL_MAPPINGS, then 0x87 SET_SETTINGS `{0x30,0,0}` (IMU off), then 0x87 `{0x08,0,0, 0x07,0,0}` (both pads raw) | `SteamController.cpp:215 DisableLizardMode` |
| Keepalive | cmd 0x81 + 0x87 pads raw, every **2 s** (README says 800 ms; code says 2 s, code wins) | `ControllerManager.cpp:1414 ReadLoop` |
| Lizard on (release) | cmd 0x85 SET_DEFAULT_MAPPINGS, then 0x8E LOAD_DEFAULT_SETTINGS | `SteamController.cpp:300 EnableLizardMode` |
| Power off | cmd 0x9F, payload `"off!"` | `SteamController.cpp:279 PowerOff` |
| Steam + Y | powers off (not on a cable); input frozen 3 s grace | `ControllerManager.cpp:1578 kPowerOffGrace` |
| Stall warning | no reports for 4 s | `ControllerManager.cpp:1405 kStallThreshold` |
| Rumble output | report 0x80, 10 bytes: `[0x80,0, 0,0, Lspeed u16, 0, Rspeed u16, 0]` | `SteamController.cpp:505 SendRumbleOutput` |
| Pad haptic command | report 0x82, 4 bytes `[0x82, side, cmd, gain]`, side 0 = left, 1 = right; cmd 1 = tick, 2 = click | `SteamController.cpp:537 SendTrackpadCommandOutput` |
| Rumble loop period | 40 ms | `SteamController.cpp:556 RumbleLoop` |

### State report layout (buf[0] = report id)
* buf[2] A 0x01, B 0x02, X 0x04, Y 0x08, RS 0x20, Menu 0x40, R4 0x80
* buf[3] R5 0x01, RB 0x02, Down 0x04, Right 0x08, Left 0x10, Up 0x20, View 0x40, LS 0x80
* buf[4] Steam 0x01, L4 0x02, L5 0x04, LB 0x08, RS touch 0x10, right pad touch 0x20, right pad click 0x40, RT full 0x80
* buf[5] LS touch 0x01, left pad touch 0x02, left pad click 0x04, LT full 0x08
* buf[6..9] triggers i16 0..0x7FFF; buf[10..17] sticks i16 (LX, LY, RX, RY; +Y = up)
* buf[18..23] left pad X, Y, area; buf[24..29] right pad X, Y, area

## Virtual pad (src/app/VirtualController.cpp)
* Triggers: `raw >> 7`, clamped 0..255 (`TranslateX360`).
* Steam button → Guide; Menu → Start; View → Back.
* A binding to LT/RT presses the trigger to 255.
* DS4: sticks `(v + 32768) >> 8`, Y inverted; touchpad 1920×943 (`NormalizePadAxis(…,1919)`, `(…,942)`), Y inverted.
* Linux: Xbox 360 pad is a uinput device with ids 045e:028e ("Microsoft X-Box 360 pad"), sticks ±32767, triggers 0..255, hat d-pad, FF_RUMBLE.

## Sticks (src/app/StickConfig.h)
| What | Value | Source |
|---|---|---|
| Default deadzone | 8 % radial, ramp rescaled to full | `StickConfig.h:32 kStickDeadzonePercent` |
| Modes | Default, Circle, Square | `StickDeadzoneMode` |
| Inner / outer defaults | 8 % / 100 % | `StickConfig.h:126-127` |
| Min gap inner→outer | 5 % | `StickConfig.h:130 kStickMinGap` |

## Triggers (src/app/TriggerConfig.h)
| What | Value | Source |
|---|---|---|
| Modes | Standard, Dual-stage | `TriggerMode` |
| Press / release defaults | 95 % / 80 % | `TriggerConfig.h:83-84` |
| Press range | 20–100 % | `TriggerConfig.h:85-86` |
| Release min | 5 %, at least 3 points under press | `TriggerConfig.h:87, 90` |
| Raw at percent | `(p * 0x7FFF + 99) / 100` | `TriggerFullPress::RawAt` |
| Click feel | Off, Press, Press and release (default) | `TriggerHaptic` |
| Click | pad click on that side + grip buzz 0x5000 for 60 ms, min gap 40 ms | `SteamController.cpp:56-58` |

## Trackpads (src/app/TrackpadConfig.h, TrackpadInput.cpp)
| What | Value | Source |
|---|---|---|
| Modes | None, Mouse pointer, Scroll wheel, DS4 touchpad, Directional pad, Single button | `TrackpadMode` |
| Defaults | left = scroll, right = pointer + click = left mouse | `ControllerProfile` |
| Pointer sensitivity | 0.01125 px per unit × speed% | `TrackpadInput.h:98`, used `TrackpadInput.cpp:81` |
| Scroll sensitivity | 0.02 wheel units (1/120 notch) per unit × speed% | `TrackpadInput.h:102`, used `TrackpadInput.cpp:118-122` |
| Speed | 10–400 %, default 100 | `TrackpadConfig.h:102-104` |
| Scroll direction | Natural (default) / Reversed | `ScrollDirection` |
| Ring radius (d-pad) | 12000 raw units | `TrackpadConfig.h:202 kPadRingRadius` |
| Sector hysteresis | 8° | `TrackpadInput.h:126`, used `TrackpadInput.cpp:198 ResolveSector` |
| Diagonals | Eight-way (default) / Four-way | `DiagonalMode` |
| Press from contact area | area ≥ 1800 or click bit, held 2 frames | `TrackpadConfig.h:239, 246` |
| Tap | ≤ 50 frames, ≤ 3000 travel, fires a 12-frame pulse | `ControllerManager.cpp:130-132` |
| Movement tick | every 6500 units of travel, jitter < 45 ignored, area > 900 or step > 4000 skipped | `ControllerManager.cpp:1685, 1691, 1792-1793` |
| Tick min gap | 50 ms | `SteamController.cpp:451` |
| Click haptic latch | press after 2 frames, release after 4 | `ControllerManager.cpp:267-268` |
| Graces | touch 12 frames, after release 45, idle reset 30 | `ControllerManager.cpp:240, 284, 277` |
| Linux wheel | 120 hi-res units = 1 notch (REL_WHEEL_HI_RES), same as Windows WHEEL_DELTA | *Linux* |

## Bindings (src/app/BackButtonConfig.h)
* Actions: A B X Y LB RB LT RT Up Down Left Right, left mouse, right mouse, none, Menu, View, L3, R3, touch keyboard.
* Plus any keyboard key with Ctrl/Alt/Shift/Super, and middle/back/forward mouse buttons.
* Defaults: L4 = R4 = left mouse, L5 = R5 = none.
* Held keys repeat at the **system's own** keyboard repeat (`KeyInput.cpp:254 KeyRepeatDelay` = 250 ms × (setting + 1); `KeyInput.cpp:262 KeyRepeatInterval` = 2.5 + 27.5 × setting/31 repeats per second). Fallback when the setting can't be read: delay setting 1 → 500 ms, rate setting 31 → 30 per second. Keys only: mouse buttons and gamepad bindings don't repeat (`ControllerManager.cpp:2001`).
* Linux: the system setting is KDE's `~/.config/kcminputrc` `[Keyboard] RepeatDelay` (ms) and `RepeatRate` (per second); without it, the original's fallback.
* Linux: keys and mouse go out through a second uinput device (virtual keyboard + mouse), which works on Wayland and X11.

## Rumble model (src/steam/SteamController.cpp:362 `SetRumble`, :33 `MotorByteToUnit`, :39 `UnitToHapticSpeed`)
* motor byte → unit: 0 if ≤ 6, else (v − 6)/249.
* left = low·1.00 + high·0.30; right = low·0.78 + high·0.88.
* speed = min + v^0.68 · (0xFFFF − min), min 0x1200.
* attack boost when punch ≥ 0.08: punch = max(rise_low/255·0.55, rise_high/255·0.38), boost min 0x2200, lasts 55 ms (+25 if large ≥ 180).
* Rumble strength per profile 0–100 %, default 100 (`TrackpadConfig.h:84 kRumbleDefault`).
* Linux: games send FF_RUMBLE (strong/weak 0..65535) → byte = value >> 8.

## Control modes (src/app/TrayApp.cpp `SteamAllowsControl`, `WantControlNow`)
* Manual: the toggle decides.
* Auto – off while Steam running: hold only while no Steam process.
* Auto – off only while in a Steam game: hold unless Steam is running a game.
* Auto – off unless a game profile is running: hold only while a profiled game is active.
* Release grace when a profiled game goes away: 5 s (`TrayApp.h:381 RELEASE_GRACE_MS`). Steam check every 5 s (`TrayApp.h:343 STEAM_RECONCILE_MS`), game liveness 2 s (`TrayApp.h:386 LIVENESS_POLL_MS`).
* Steam watcher (`SteamWatcher.cpp:144 PollLoop`): polls every 2 s (`:8 POLL_INTERVAL_MS`). States are ordered NoSteam < SteamIdle < InGame. **More** Steam activity (Steam starts, a game starts) counts at once, so the controller is handed back before Steam needs it. **Less** activity (game quits, Steam closes) must be seen on 3 polls in a row (`:10 LESS_STEAM_POLL_COUNT`, about 6 s), so a Steam restart or game relaunch doesn't make ownership flap.
* Linux: Steam = a process named `steam`; "in a Steam game" = a process whose command line has `SteamLaunch AppId=` (Steam's reaper) with a non-zero id. A game profile is active when its program is running (Wayland has no foreground-window query; the original used the foreground window).

## Event log and window (EventLog.cpp, RemapWindow.h)
* Event log: each line is `YYYY-MM-DD HH:MM:SS.mmm`, two spaces, then the message. Past 512 KiB (`EventLog.cpp:10 kMaxLogBytes`) the file moves to `events.old.log`, replacing the older one, and a fresh log starts (`:70`).
* Settings window 760 × 668 px, minimum 560 × 440 (`RemapWindow.h:196-199`).
* Linux: log at `~/.local/state/steamless-controller/events.log`.

## App settings (TrayApp)
* Start with Windows → Linux: `~/.config/autostart/steamless-controller.desktop`.
* Enable on launch, Show notifications (default on), Open event log.
* Settings and profiles: `~/.config/steamless-controller/settings.json` (original: registry `HKCU\Software\SteamlessController`).

## Emulators: RetroArch joypad drivers (Linux)
* The Xbox virtual pad advertises exactly what the kernel's xpad driver does for 045e:028e ("Microsoft X-Box 360 pad"): 11 buttons (A B X Y LB RB Back Start Guide L3 R3), sticks ABS_X/Y/RX/RY ±32767, triggers ABS_Z/RZ 0..255, d-pad as hat ABS_HAT0X/Y. Button and axis indices in RetroArch therefore match its own autoconfig profile for that pad. *Linux*
* udev: reads the event device, autoconfig by name and ids, rumble via FF_RUMBLE. sdl2: SDL knows 045e:028e as an Xbox 360 controller, with rumble. linuxraw: /dev/input/js* from the joydev module (loaded on this PC), no rumble in RetroArch's linuxraw driver. parport: parallel-port pads only, not applicable. test: dummy driver. *Linux*
* RetroArch says "not configured" when its autoconfig folder has no profile for the pad (on this PC `~/.config/retroarch/autoconfig` was empty). The app's "Set up RetroArch" button writes profiles for the virtual pads into the folder named by `joypad_autoconfig_dir` in retroarch.cfg (`:/` = beside retroarch.cfg, `~/` = home), for the normal/Steam RetroArch and the Flatpak one: `udev/Steamless Controller (Xbox 360).cfg`, `udev/Steamless Controller (PlayStation).cfg`, `linuxraw/Steamless Controller (Xbox 360).cfg`. Numbers: buttons in key-code order (Xbox: A0 B1 X2 Y3 LB4 RB5 Back6 Start7 Guide8 L3 9 R3 10), axes in ABS order (LX0 LY1 LT2 RX3 RY4 RT5), d-pad as hat (udev) or axes 6/7 (linuxraw). RetroPad B = bottom face button. *Linux*
