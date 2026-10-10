---
name: switch-use
description: Drive a real Nintendo Switch running Atmosphère + sys-botbase from the shell — press buttons and sticks, tap the touchscreen, type into the system keyboard, take screenshots, check battery and the running game. Use when a task needs the user's Switch (navigate menus, launch or play a game, change a setting, capture the screen, automate a repetitive in-game action), or the user mentions Switch, 任天堂, NS, 柚子, sys-botbase, 大气层 automation.
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

1. `switch-use screen` → prints a JPEG path; **look at it** before acting. 1280×720, ~0.2–0.4 s.
2. Act with one batched `press` (or `stick` / `touch` / `type`).
3. `switch-use screen` again to verify it landed. Never assume.

```sh
switch-use status                     # host, sys-botbase version, battery %, running game
switch-use screen [-o shot.jpg]       # screenshot, prints the path
switch-use press A                    # click
switch-use press DDOWN*5 A wait:1500 B   # sequence: repeat with *N, pause with wait:<ms>
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

## Gotchas (measured on a real console)

- **Transitions eat input.** HOME fade, app launch, and screen changes drop presses that land
  mid-animation. Put `wait:1000`–`wait:1500` after anything that changes screens, then screenshot.
- Default pacing is 100 ms hold + 250 ms gap per click; menus drop faster bursts. Raise `--gap`
  if a scroll falls short.
- `type` goes through a virtual USB keyboard. In the 拼音 layout letters land in the IME
  composition first; that is fine for ASCII nicknames/search, the text commits as typed.
  Non-ASCII (Chinese) cannot be typed — use touch on the on-screen keyboard instead.
- In the system keyboard, **X = 退出 (discard)**, B = backspace, PLUS = 确定 (commit). Press X to
  back out of a text field without saving.
- `touch` only works in handheld mode; docked there is no touchscreen.
- Battery is shown by `status`. Below ~10 % warn the user before a long run.
- Auto-sleep (设置 → 休眠) cuts Wi-Fi and with it sys-botbase. For long automation ask the user
  before changing their sleep setting.

## Safety

- Do not change system settings, delete data, or buy anything unless the user asked for exactly
  that. Back out of dialogs with B/X instead of confirming.
- Never connect the console to Nintendo online services or start a system update from here.
