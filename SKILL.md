---
name: switch-use
description: Drive a real Nintendo Switch running Atmosphère + sys-botbase from the shell — read the text on screen (OCR), tap a label, wait for text to appear, press buttons and sticks, type into the system keyboard, take screenshots, check battery and the running game. Use when a task needs the user's Switch (navigate menus, launch or play a game, change a setting, capture the screen, automate a repetitive in-game action), or the user mentions Switch, 任天堂, NS, 柚子, sys-botbase, 大气层 automation.
allowed-tools: Bash(switch-use:*)
---

# switch-use

Single Rust binary that talks to **sys-botbase** (an Atmosphère sysmodule) on TCP 6000 over the
LAN. No Python, no daemon.

**Install / self-heal:** if `switch-use` is missing, install it, then retry:

```sh
curl -fsSL https://raw.githubusercontent.com/leeguooooo/switch-use/main/install.sh | sh
```

## Finding the Switch

`switch-use discover` scans this machine's /24 for sys-botbase and saves the address to
`~/.config/switch-use/host`. Every other command uses `--host`, then `$SWITCH_USE_HOST`, then the
saved address, and rescans by itself if the saved one stopped answering (DHCP moved it).

If nothing answers: the Switch is asleep (Wi-Fi drops in sleep), not on Wi-Fi, or booted into
something without sys-botbase (stock firmware, hekate). Ask the user to wake it and boot
**大气层虚拟系统 (emuMMC)**. Never ask them to go online from sysMMC.

## The loop

The Switch has no accessibility tree. `elements` is the stand-in: it grabs the screen and runs
macOS Vision OCR (Chinese, Japanese, English), listing every line of text with the point to
touch. Read text first; open the screenshot only when text is not enough.

0. `switch-use status` first. If a game is running, ask before pressing HOME or closing it,
   unless the user has already authorized that interruption.
1. `switch-use elements` → `[i] text (x,y)` lines plus the screenshot path, ~0.5 s.
   A trailing `?` marks a low-confidence read (pixel fonts, box art): treat it as a hint.
2. Act in one batch: `press … see:<text>` / `tap <label>` / `stick` / `type`, and add
   `--observe` so the reply is the settled screen's text — no separate read needed.
3. Check that reply against what you expected. Never assume a press landed.

```sh
switch-use status                     # host, sys-botbase version, battery %, running game
switch-use elements [--json]          # text on screen + center points + screenshot path
switch-use elements --from shot.jpg   # OCR a saved screenshot
switch-use --observe press A          # any action + the settled screen's text afterwards
switch-use tap 设置 [--nth 1]          # touch the text matching a label (handheld only)
switch-use wait 继续 [--absent] [--timeout 10000]   # until text appears / disappears
switch-use screen [-o shot.jpg]       # screenshot only, prints the path
switch-use press A                    # click
switch-use press DDOWN*5 A see:系统设置 B   # *N repeats; see:<text> waits until it shows
switch-use press A gone:加载中 wait:500 # gone:<text> waits until it is gone; wait:<ms> sleeps
switch-use hold ZL ZR --ms 800        # hold buttons together
switch-use stick L up --ms 400        # or: stick R 0.5 -1 --ms 300  (x y in -1..1)
switch-use touch 640 360 [--hold 500] # handheld only, 0..1280 x 0..720
switch-use swipe 200 600 1000 600     # finger path x1 y1 x2 y2 ...
switch-use type "Hi Leo-2026!" [--enter]  # ASCII into the system keyboard
switch-use home                       # HOME menu (waits out the fade-in)
switch-use display off|on             # backlight, saves battery during long runs
switch-use raw <sys-botbase command> [--reply]
```

Buttons: `A B X Y L R ZL ZR PLUS MINUS LSTICK RSTICK HOME CAPTURE DUP DDOWN DLEFT DRIGHT`
(aliases: `UP DOWN LEFT RIGHT`, `+ -`, `LS RS`).

### Don't → do instead

| Don't | Do instead |
|---|---|
| Screenshot after every step | `--observe` on the action, or `elements` |
| Guess touch coordinates from an image | `tap <label>`; the coordinates in `elements` are touch coordinates |
| `wait:1500` after a screen change | `see:<text>` / `gone:<text>` in `press`, or `switch-use wait` |
| Repeat a `tap` whose label was not found | Use the `did you mean` text or re-read `elements` |
| Trust OCR for which item is selected | Look at the screenshot (see below) |

**What OCR cannot tell you:** which item has focus. Most Switch menus are driven by the D-pad and
show the selection as a highlight frame (the cyan border on HOME), not as text. After moving the
selection, open the screenshot path from `elements`/`--observe` to confirm before pressing A.
In-game pixel fonts and box art also read poorly. Lines marked `?` may be wrong.

## Gotchas (measured on a real console)

- **Transitions eat input.** HOME fade, app launch, and screen changes drop presses that land
  mid-animation. After anything that changes screens, wait for the next screen's text with
  `see:<text>`; use `wait:1000`–`wait:1500` only where no stable text shows (game transitions).
- **Wake can leave the virtual controller unresponsive.** If the screen works but buttons
  do nothing after sleep, run `switch-use raw detachController`, then retry one button
  with `--observe` to verify. The next input reconnects it; do not blindly repeat a batch.
- **Applet mode can crash homebrew.** DBI crashed when launched from Album on the test
  console; the cause is unconfirmed. Prefer full application mode for DBI/RetroArch
  (hold R while launching a game), after permission to close any running game.
- Default pacing is 100 ms hold + 250 ms gap per click; menus drop faster bursts. Raise `--gap`
  if a scroll falls short.
- `type` goes through a virtual USB keyboard. In the 拼音 layout letters land in the IME
  composition first; that is fine for ASCII nicknames/search, the text commits as typed.
  Non-ASCII (Chinese) cannot be typed — use touch on the on-screen keyboard instead.
- In the system keyboard, **X = 退出 (discard)**, B = backspace, PLUS = 确定 (commit). Press X to
  back out of a text field without saving.
- `touch` and `tap` only work in handheld mode; docked there is no touchscreen.
- OCR (`elements`, `tap`, `wait`, `see:`, `--observe`) needs macOS (Vision). On Linux use
  `screen` and look at the image.
- Battery is shown by `status`. Below ~10 % warn the user before a long run.
- Auto-sleep (设置 → 休眠) cuts Wi-Fi and with it sys-botbase. For long automation ask the user
  before changing their sleep setting.

## Safety

- Do not change system settings, delete data, or buy anything unless the user asked for exactly
  that. Back out of dialogs with B/X instead of confirming.
- Never connect the console to Nintendo online services or start a system update from here.
