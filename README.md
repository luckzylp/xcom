# XCOM Serial Debug Assistant

> English | [简体中文](README_ZH.md)

A cross-platform serial port debugging tool written in **Rust + egui**, reimplementing the features and UI of the ALIENTEK **ATK-XCOM v3.0**, running on **Windows / Linux / macOS**.

The UI contains **no advertisements** and additionally supports **automatic character-encoding detection** and **dark/light themes** that the original lacks.

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ File  Edit  View  Tools  Help                                                │
├───────────────┬──────────────────────────────────────────┬───────────────────┤
│ Serial Config  │                                          │ Multi-Send | Proto │
│ Port   [▾]    │          Receive Area (black/green)       │                   │
│ Baud   [▾]    ├──────────────────────────────────────────┤  # Content  Note  │
│ Data   Stop   │                                          │  0 AT      Query  │
│ Parity Flow   │                                          │  1 ...            │
│ DTR RTS       │                                          │                   │
│ [Open Port]   │                                          │                   │
│ Statistics    ├──────────────────────────────────────────┤  Loop Send Keypad │
│               │ Send Area                                 │                   │
│               │  [Hex Send] [Newline] [Timed interval] [Timed] │                   │
│               │  [Send File] [History▾] [Send Ctrl+Enter]   │                   │
├──────────────┴──────────────────────────────────────────┴───────────────────┤
│ ● /dev/ttyUSB0 [115200-8N1] Enc Auto·UTF-8 Tx:0 Rx:0 CTS● DSR○ DCD○ Time     │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## Feature Comparison

Features are aligned with the official documentation ([ATK-XCOM v3.0 manual](http://47.111.11.73/docs/tool/ruanjian/ATK-XCOM.html)). The table below compares item by item:

| Official Feature | This Implementation | Notes |
| --- | --- | --- |
| Auto-scan available serial ports, show detailed port info | ✅ | Background thread enumeration; shows VID:PID / vendor / product / serial number; hot-plug support |
| Ultra-high baud rates, custom baud rates | ✅ | Standard + persistent custom choices, 50–12000000 |
| Stop bits 1 / 1.5 / 2 | ⚠️ | See [Known Limitations](#known-limitations) |
| Data bits 5 / 6 / 7 / 8 | ✅ | |
| Odd, even, no parity | ✅ | |
| Hot-plug serial ports | ✅ | Auto-closes and prompts on disconnect |
| Hex display (can auto-convert existing data) | ✅ | Text ⇄ HEX toggle re-renders historical data |
| Display modes: black bg green text / white bg black text | ✅ | Controlled by theme; custom foreground/background colors also available |
| Echo sent content | ✅ | Sent content shown in a different color in the receive area; toggle under View → Display or in the receive area's right-click menu |
| RTS / DTR control | ✅ | Switchable in real time while port is open |
| Timestamps (frame-delimited by CR/LF) | ✅ | An incomplete frame is shown live at the end of the receive area with its current timestamp, which is fixed once CR/LF completes it |
| Direction markers on every row | ✅ | Coloured `->` for received data, `<-` for sent data; toggle under View → Display or in the receive area's right-click menu |
| Save receive window as any format file | ✅ | Save as text or raw binary |
| Live log file while receiving | ✅ | File → Log file…; the name pattern supports `%Y %M %D %h %m %s` and more, and an existing file is appended to |
| Clear receive (also resets Tx/Rx counters) | ✅ | |
| Single send: any characters / hex / auto-split bytes | ✅ | `0102`, `01 02`, `0x01,0x02`, `01-02` all accepted |
| Send newline | ✅ | CR / LF can be individually checked |
| Timed send + custom interval | ✅ | Interval is set before ticking, then locked; closing the port cancels it |
| Send any format file, abort anytime, progress bar | ✅ | Chunked send, can be cancelled |
| Ctrl+Enter shortcut to send | ✅ | |
| Clear sent (also resets Tx/Rx counters) | ✅ | |
| Status bar: Tx/Rx byte counts, CTS/DSR/DCD, system time | ✅ | Also shows RI indicator |
| Multi-send: fixed pool of 100 slots | ✅ | Page size follows the tool panel height and the page count is derived; entry numbers are global and continuous across pages; entries have note, enable, HEX, newline toggles |
| Multi-send: first/prev/next/last page | ✅ | |
| Multi-send: link to numeric keypad 0–9 | ✅ | |
| Multi-send: auto loop send | ✅ | Sends only checked entries; supports **infinite loop** or **specified loop count**; a round is paced entirely by each entry's own delay, with no separate round gap |
| Multi-send: send all checked entries at once | ✅ | "Send Checked" button, each sent once in order |
| Multi-send: import/export entries | ✅ | JSON format, easy to edit and version-control |
| Protocol transfer (Modbus RTU variant, Send-Ack) | ✅ | See [Protocol Transfer](#protocol-transfer) |
| Protocol: retry count, frame period, checksum/XOR/CRC8/CRC16 | ✅ | Algorithms byte-for-byte identical to the official implementation |
| Protocol: single / auto / file send | ✅ | |
| Protocol: response area shows raw frames / auto-wrap | ✅ | |
| **UI advertisements** | ➖ | **Removed as requested** |

### Additions in This Implementation

- **Automatic encoding detection**: No need to manually guess GBK / UTF-8 / Shift_JIS.
- **Dark + Light themes** (can also follow the system); the dark theme uses a pure black background, with menus and input fields kept a few steps above it for contrast.
- **Chinese/English UI toggle**: switchable at any time from **View → Language**; follows system `LANG` on first launch; choice is persisted.
- **Receive area text selection**: drag-select with mouse, Ctrl+C copy, Ctrl+A select all (based on glyph metrics, CJK full-width characters select correctly).
- **Keyword highlighting**: add keywords and colours under View → Display → Keyword highlighting…; every row has its own enable switch (new rules start off) and a `#N` number. Each rule is a literal or, with the `.*` toggle, a regular expression (invalid patterns are flagged and skipped). There is no fixed rule limit: a long list scrolls inside the table while the Add row stays visible. Rules persist in the settings file, and longer matches win when they overlap.
- **Live log files**: File → Log file… writes every received byte to disk while the session runs. The file name is expanded once from a pattern when logging starts, with `%Y %y %M %D %h %m %s %S %f %H %%` tokens (`%H` is the serial port name); an existing file is appended to, and the folder and pattern persist.
- **A grouped, submenu-only View menu**: every entry under View is a submenu — Panels (configuration/tool/send area), Display (display mode, markers, timestamps, wrapping, auto-scroll, echo, HEX layout, plus separate Font settings, Receive colours and Keyword highlighting windows), Encoding, Theme and Language.
- **Separate appearance windows**: fonts, receive colours and keyword rules each have their own resizable window instead of sharing one settings page. Line spacing now lives with the fonts, and the colours window shows a live sample using the effective palette.
- **Resizable, centered dialogs**: the font settings, receive colours, keyword highlighting, log file, shortcuts, about and custom-baud windows open centered in the app window, then can be dragged and resized (down to a sensible minimum).
- **Virtualized log view**: Only visible lines are rendered, staying smooth during long high-speed reception.
- **Coloured direction markers**: each row starts with an arrow — `->` for data received from the device (cyan), `<-` for data we sent (gold) — so the direction of every line is obvious at a glance. Timestamps follow the marker; the markers can be switched off under View → Display or in the receive area's right-click menu. Plain ASCII glyphs are used deliberately: they exist in every font, unlike Unicode arrows, which are missing from some CJK faces or render double-width.
- **Configurable interface, receive-area and send-area fonts/sizes**, with matched CJK/Latin baselines by default.
- **Adjustable receive area line spacing** (in the font settings window); auto-wrap width follows window resize and the selected font.
- **Collapsible side panels**: drag the configuration panel's splitter all the way left (or the tool panel's all the way right) and it snaps shut, handing the width to the receive area. A hairline stays at the edge, with a chevron on hover to open it again; the "Configuration panel / Tool panel" items under **View** are the same state. The centre column keeps a 320px floor, so each panel's maximum is a *soft* one that only applies when the window can spare the room.
- **The multi-send panel marks the entry being sent**: during "send checked" or looping, the entry in flight is highlighted with a leading colour bar, and the send status bar below the table shows `Sending #3 (3 / 7)` using the same global number as the table's `#` column (the run spans the whole pool, not just the current page, and the mark appears only on the page currently showing that entry). The mark clears when the run ends, is stopped, the port closes, or the app restarts. "Loop sending" is a run state rather than a saved preference, so it never survives into a state where it claims to be looping but isn't.
- Configuration file persistence; restores last serial and display options after restart.

---

## Installation and Running

### Prerequisites

You only need a recent Rust toolchain (**1.90+**, `edition 2024`):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Linux also requires graphics development libraries (most desktop distros already include them):

```bash
# Debian / Ubuntu
sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev \
                 libx11-dev libgl1-mesa-dev
# Fedora
sudo dnf install gcc pkgconf-pkg-config libxkbcommon-devel wayland-devel \
                 libX11-devel mesa-libGL-devel
```

### Build and Run

```bash
cargo run --release
```

The binary is at `target/release/xcom` (`xcom.exe` on Windows).

### UI Language

The **View → Language** submenu switches between **Simplified Chinese** and **English** instantly, without restarting.

- On first launch, it detects from `LC_ALL` / `LC_MESSAGES` / `LANG` (`zh*` → Chinese, otherwise English), and persists the result; thereafter the user's choice takes precedence.
- The implementation uses a gettext-style approach: **the Chinese text is the translation key**, wrapped as `tr("打开串口")`. This makes batch conversion mechanical and diffs readable; if a translation is missing, it falls back to the Chinese original rather than showing a blank label.
- Language is a **process-level global** rather than a receiver passed down through layers, so call sites in closures and free functions just write `tr(...)` without holding an `App` reference.
- A **guard test** scans all source code for string literals passed to `tr()`; any missing English entry causes the test to fail. A test sample is fed to the scanner to prevent it from silently passing if the scanner itself breaks.

### Window Frame and Rounded Corners

On Linux + Wayland, the app draws its own client-side frame:

- eframe depends on winit with `default-features = false`, which **silently disables** winit's `wayland-csd-adwaita`. GNOME's Mutter doesn't implement the `xdg-decoration` protocol, so by default winit falls back to a bare-bones frame: square corners, window buttons on the right, ignoring the desktop's `button-layout` setting.
  This project's `Cargo.toml` explicitly re-enables that feature, so `sctk-adwaita` draws a standard Adwaita frame and reads `org.gnome.desktop.wm.preferences button-layout` via XDG Desktop Portal, so the minimize/maximize/close positions match the desktop setting (left on GNOME by default).
- The dialogs the app draws itself ("Custom baud rates", "About XCOM") are not system windows, and egui's built-in title bar hard-codes its close button on the right. Both therefore draw their own title bar and read the same desktop `button-layout` setting (via `gsettings`, probed once per process), putting the close button on the left when the desktop does and on the right otherwise; if the setting cannot be read, it stays on the right. See `docs/adr/0001-dialogs-own-their-chrome.md`.
- The window's bottom rounded corners are drawn by the app: `src/ui/window.rs` overlays a rounded-corner background across the entire window and sets `clear_color`'s alpha to 0. On compositors that support alpha, the corners are transparent (rounded); on those that don't, corners use the panel background (merely square, never black).
- Windows 11 / macOS use native system frames and rounded corners, unaffected by the above logic.

### Platform Notes

| Platform | Notes |
| --- | --- |
| **Linux** | Add your user to the `dialout` group for sudo-free serial port access: `sudo usermod -aG dialout $USER` (re-login required). File dialogs use XDG Desktop Portal; if the system lacks a portal, the path selection before "Save Raw Data" may not respond — use the config file or run in a supported environment. |
| **Windows** | Release builds don't pop up a console window. Requires the VC++ runtime (built into modern Windows). |
| **macOS** | On first launch, allow the app in "System Settings → Privacy & Security"; serial devices look like `/dev/tty.usbserial-*`. |

### CJK Fonts

The UI is in Chinese; on startup the program automatically searches for CJK fonts on the system, in this order:

1. Common font paths (e.g., `/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc`, `C:\Windows\Fonts\msyh.ttc`, `/System/Library/Fonts/PingFang.ttc`);
2. Via `fontdb` scanning system fonts, matching Noto Sans CJK / Source Han Sans / Microsoft YaHei / PingFang / WenQuanYi, etc.

If none are found, the program still runs but Chinese characters appear as squares, with a status bar prompt to install a font (on Linux, install `fonts-noto-cjk` or `fonts-wqy-microhei`).

The default interface stack prefers a font that contains both Latin and CJK glyphs (such as Noto Sans CJK SC, Microsoft YaHei, or PingFang SC). This avoids the vertical baseline mismatch that occurs when ASCII comes from one font and Chinese from a fallback. Users can still select any installed interface, receive, or send font in **View → Display → Font settings…**.

---

## Adaptive Encoding

Text encoding from serial devices is highly inconsistent: 8051/STM32 firmware typically uses **GBK**, ESP-IDF outputs **UTF-8**, and Japanese modules often use **Shift_JIS**. The original tool requires manual selection — pick wrong and you get garbled text.

This implementation provides both **auto-detection** (default) and **manual selection**:

- Auto-detection progressively analyzes the data stream and locks the result:
  1. Pure ASCII is valid for all candidate encodings and never triggers a switch;
  2. Valid multi-byte UTF-8 is immediately identified as UTF-8 (this avoids `chardetng` misidentifying short Chinese UTF-8 as GBK);
  3. After accumulating enough data, re-detection only happens when the current encoding shows obvious garbling (replacement character ratio exceeds 1%);
  4. It never switches to a candidate encoding that decodes worse.
- An incremental decoder handles multi-byte characters split across data packets — no half-characters.
- When switching encoding or HEX/text mode, **received historical data is re-rendered under the new setting** (subject to the cache limit).
- Receive and send encodings can be switched at any time under **View → Encoding**; send encoding defaults to "follow receive encoding" but can be set separately.

The status bar shows the active encoding, e.g., `Auto·UTF-8`.

---

## Protocol Transfer

The protocol is a half-duplex **Send-Ack** mode derived from Modbus RTU. Frame format:

```
+----------+----------+----------+----------+-------------+----------------+
| Device   | Function | Sequence | Data     | Data        | Checksum        |
| Address  |          |          | Length   | Field       |                 |
| 1 byte   | 1 byte   | 1 byte   | 1 byte   | 0..255 bytes| 1 or 2 bytes    |
+----------+----------+----------+----------+-------------+----------------+
```

- **Data length** counts only the data field, not the header or checksum; minimum frame 5 bytes, maximum 261 bytes.
- **Checksum covers all preceding bytes** (including the data length field).
- CRC16 is **CRC-16/MODBUS** (reflected polynomial `0xA001`, initial value `0xFFFF`), **low byte first**, consistent with Modbus RTU.
- Checksum (SUM) is the **two's complement** of the byte sum, i.e., the byte sum of the entire frame mod 256 equals 0.
- CRC8 is **CRC-8/MAXIM** (reflected polynomial `0x31`, initial value `0x00`); the checksum of `"123456789"` is `0xA1`.
- No start/stop delimiters: response frames are bounded by **bus idle time**, threshold = `320000 / baud_rate + 2` milliseconds.

Retransmission: the frame period is both the auto-send interval and the **retransmission timeout**. On timeout, it retransmits with the **same frame sequence**, up to `max retries` times, then reports "device no response". Only new logical frames increment the sequence (starting from 0, so the first frame's sequence is `1`).

Response validation: only the **frame sequence** and **checksum** (and length validity) are verified. Device address and function are marked as "arbitrary" per the official example — shown for display only, not validated.

> Frame format based on: the frame format diagram on the official v3.0 manual page, the same table in the official forum announcement, and decompiled `ManageTransportProtocol.cs` / `CheckTransportProtocol.cs` from the official XCOM binary (V2.3), cross-validated with real captured frames.
> The v3.0 page embeds the same frame format diagram and text and is verbatim identical to the 2013 announcement, so we infer v3.0 did not change this format (this is inference, not direct evidence).

### A Practical Constraint

The frame period **must be greater than the response frame-delimiting time**, otherwise retransmission fires before the response is recognized. The program warns when the frame period is too short.

---

## Keyboard Shortcuts

| Shortcut | Action |
| --- | --- |
| `Ctrl + Enter` | Send the content of the send area |
| `Ctrl + O` | Open / close serial port |
| `F5` | Refresh the port list |
| `Ctrl + S` | Save receive data |
| `Ctrl + Shift + S` | Save raw data |
| `Ctrl + Q` | Quit |
| `Ctrl + C` | Copy selected content in receive area |
| `Ctrl + A` | Select all in receive area |
| `1`–`9`, `0` | Send corresponding entry when "link numeric keypad" is checked in multi-send panel |

Right-click inside the receive area for Copy selection, Select all, Pause/Resume display, HEX display, Echo sent, Direction markers, Timestamps, Wrap long lines, Auto-scroll to newest, Keyword highlighting… and Clear receive.

---

## Desktop Integration (Linux)

`packaging/xcom.desktop` provides a desktop entry (Chinese and English names, categories, keywords). The `StartupWMClass=xcom` must match `ViewportBuilder::with_app_id("xcom")` in `src/main.rs`, otherwise the compositor can't associate the window with the entry, and the taskbar won't get the icon or correct name.

```bash
cargo build --release
sudo install -Dm755 target/release/xcom /usr/local/bin/xcom
sudo install -Dm644 packaging/xcom.desktop /usr/share/applications/xcom.desktop
sudo install -Dm644 packaging/xcom.png /usr/share/icons/hicolor/256x256/apps/xcom.png
sudo update-desktop-database /usr/share/applications
sudo gtk-update-icon-cache -f /usr/share/icons/hicolor 2>/dev/null || true
```

For current-user-only installation (no root needed), use the script to do it all at once:

```bash
cargo build --release
./packaging/install-user.sh          # Uninstall: --uninstall
```

The install script first does a **startup-time self-check** (~130 ms measured), then installs to Debian-expected user-level locations, and finally **checks whether `TryExec` resolves to a command**:

```
~/.local/bin/xcom                                  # on PATH, so Exec=xcom works
~/.local/share/applications/xcom.desktop
~/.local/share/icons/hicolor/<size>x<size>/apps/xcom.png   # 48/64/128/256/512
```

**`StartupNotify` must be `false` (the template already does this).** It means "the app will send a handshake signal when startup completes" (`_NET_STARTUP_INFO` / activation token). winit makes this handshake **an opt-in API the app must call**, and egui/eframe never calls it. If declared `true`, the Dock stays in "starting" state until the promise times out — **the window is already visible but the taskbar shows a generic icon for several seconds before associating it**. Declaring `false` lets the window match by app id immediately.

**`TryExec` is the most common pitfall**: it resolves on **PATH**, not relative to `Exec`. Setting `Exec` to an absolute path while leaving `TryExec` as a bare name causes `TryExec` to fail, and **GNOME ignores the entire entry** — no menu item, no icon, even though all icon files are present. The template has this line **commented out by default**.

Icons are drawn at each size individually by `packaging/make-icons.py` (rather than scaling a single bitmap), to stay crisp at 48px:

```bash
python3 packaging/make-icons.py
```

`packaging/xcom.desktop` is a **key-by-key annotated template** you can adapt directly:

- After editing, validate with Debian's built-in validator — **it must produce no output**:
  ```bash
  desktop-file-validate packaging/xcom.desktop
  ```
- `Categories` may have only **one main category**. This project uses `Development;Electronics;` — main category `Development` plus additional category `Electronics`. The previously used `Development;Electronics;Utility;` had two main categories, causing the app to appear twice in the menu, with the validator flagging it.
- `Icon` must be an **icon name**, never with a path or extension; the icon is resolved through the hicolor theme.
- All list-type keys (`Categories`, `Keywords`) must end with `;`.
- `StartupWMClass` on Wayland must equal the app id, i.e., `with_app_id("xcom")` in `src/main.rs`; on X11 it equals the window's `WM_CLASS`.

## Configuration File

Settings are saved at:

| Platform | Path |
| --- | --- |
| Linux | `$XDG_CONFIG_HOME/xcom/settings.json` or `~/.config/xcom/settings.json` |
| Windows | `%APPDATA%\xcom\settings.json` |
| macOS | `~/Library/Application Support/xcom/settings.json` |

The file is readable JSON and can be edited directly. Fields use `#[serde(default)]`, so **configs written by older versions still load in newer versions** (missing fields take default values). Writes use "temp file + rename", so an interruption never leaves a partial file; a corrupted config falls back to defaults rather than failing to start.

---

## Known Limitations

1. **1.5 stop bits**: Windows DCB supports `ONE5STOPBITS`, but the Rust `serialport` crate doesn't expose it; on Linux/macOS, termios can only express 1.5 stop bits with `CSTOPB + CS5` when **data bits = 5**. Therefore this option **explicitly prompts "treated as 2 stop bits"** on unsupported combinations, rather than silently opening with wrong parameters.
2. **File dialogs** depend on `rfd`: on Linux it goes through XDG Desktop Portal, which may not get a path in minimal environments without a portal. "Save Raw Data" is provided as an alternative, and entry import/export uses JSON text to bypass the graphical dialog.
3. **Protocol transfer** frame format is based on decompiled V2.3 official implementation (see above); v3.0 binary has not been published.
4. The receive cache default limit is 32 MiB / 200k lines; oldest data is dropped when exceeded — this keeps memory bounded during long-running sessions. The limit can be adjusted in the "Statistics" panel.

---

## Development

```bash
cargo test              # 255 tests
cargo clippy --all-targets
cargo fmt
```

Test coverage:

- Hex parsing/formatting, four checksum algorithms (including standard test vectors and real captured frames);
- Log store line splitting, CRLF across packets, auto-wrap (including CJK double-width), timestamp frame-delimiting, direction markers (including re-rendering when switched off and wrap continuations carrying none), cache trimming;
- Encoding auto-detection (UTF-8 / GBK Chinese streams) and cross-packet multi-byte decoding;
- Protocol state machine: response, retransmission, file chunking and ordering;
- **Real PTY loopback**: opening a device, sending/receiving bytes, write counters, re-opening, handshake lines — covering reader/writer threads and event channels;
- **Headless UI layout**: using `Context::run_ui` to render a full frame, covering empty logs, both themes, all display modes, bilingual, narrow-window combinations, and asserting primitives are actually produced;
- **Multi-send column widths**: asserting column widths grow as the panel widens and their sum never exceeds the panel width;
- **Loop count**: asserting that a configured N-count stops after N rounds, infinite loops don't stop, no checked entries causes a stop, and a round is paced by the last entry's own delay;
- **Config backward compatibility**: asserting that old config files missing new fields still load (otherwise all settings would silently reset);
- **UI layer no untranslated text**: scanning `src/ui/`, any Chinese literal not wrapped in `tr()` causes the test to fail;
- **Splitter drag**: simulating real pointer event sequences, asserting all three splitters can be dragged both ways, content changes accordingly, overflow is clamped, and sizes persist;
- **Panel collapse**: driving the same real pointer sequences to assert a panel dragged off the window edge folds away while keeping the width to restore, that the edge strip reopens it by click and by drag, that the menu checkbox agrees, and that the centre column keeps its minimum width;
- **Dialog title bars**: asserting both app-drawn dialogs put the close button where the desktop's `button-layout` says, plus parser unit tests (close on the left only when the left group names it; a value with no colon counts as unset and stays right);
- **Entry pool and pagination**: asserting the fixed pool of 100 global slots, that the page size derives the page count and the current page is clamped, that sparse storage is padded back to the full pool on load, and that the legacy page format still imports;
- **Multi-send marker**: asserting the mark lands on the right row, that the status line names the entry and its place in the round, that the mark appears only on the page showing the entry, and that ending the run or closing the port clears both the mark and the loop switch — including that `loop_enabled` and the page number are never written to, nor read back from, the config file;
- **Real serial send/receive**: opening a port with PTY, driving timed send and asserting the peer receives repeated payloads, and that send echo indeed enters the receive area without polluting the receive stream;
- **Config backward compatibility**: asserting that old configs missing new fields still load.

### Code Structure

```
src/
  main.rs          Entry point, font installation
  app.rs           App state, serial/timer event pump, shortcuts
  logstore.rs      Receive log: byte stream → display lines (incremental, bounded, re-renderable)
  encoding.rs      Encoding table, auto-detector, streaming decoder
  hex.rs           Lenient hex parsing and formatting
  multi.rs         Multi-send entry pool, pagination and import/export
  settings.rs      Config persistence
  theme.rs         Dark/light themes and receive area colors
  fonts.rs         CJK font discovery
  serial/          Serial config, port enumeration (hot-plug), link and read/write threads
  protocol/        Checksum, frame format, transfer state machine
  ui/              log_view (virtualized selectable log), panels (various panels),
                   dialog (title bars for the app's own dialogs), window (window background)
```

## License

MIT
