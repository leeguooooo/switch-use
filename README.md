# switch-use

**Let AI agents drive a real Nintendo Switch.** Read the text on screen, tap a label, wait for a
screen, press buttons and sticks, type, take screenshots — from the command line or from Claude
Code, Codex or any agent that can run a shell command. Computer-use, but for the Switch.

**让 AI agent 操作真实的任天堂 Switch。** 读屏幕文字（OCR）、按文字点击、等待画面出现、按键和摇杆、
触摸、键盘输入、截图。主机跑大气层（Atmosphère）+ sys-botbase，电脑和主机在同一个局域网即可。

- One ~1 MB Rust binary, no daemon, no Python. macOS and Linux.
- Talks to [sys-botbase](https://github.com/olliz0r/sys-botbase) over TCP on your LAN.
- Screen text via macOS Vision OCR (Chinese, Japanese, English) with the point to touch.
- Ships an agent skill ([SKILL.md](SKILL.md)); part of the [`*-use` family](https://github.com/leeguooooo/plugins)
  (iphone-use, chrome-use, mail-use, …).
- Write-up with real-console stories: [English](https://blog.leeguoo.com/en/posts/switch-use/) ·
  [中文](https://blog.leeguoo.com/zh/posts/switch-use/).

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/leeguooooo/switch-use/main/install.sh | sh
```

Apple Silicon Macs and Linux (x86_64, arm64) get a prebuilt binary from GitHub Releases. Intel
Macs: `cargo install --git https://github.com/leeguooooo/switch-use`. Update later with
`switch-use upgrade`.

As an agent skill:

- **Claude Code:** `/plugin marketplace add leeguooooo/plugins`, then
  `/plugin install switch-use@leeguooooo-plugins` (or the whole family: `use-family`).
- **Codex / other agents:** `npx skills add leeguooooo/switch-use`.

On the Switch (Atmosphère, booted into emuMMC):

1. Copy [sys-botbase](https://github.com/olliz0r/sys-botbase/releases) to the SD card
   (`atmosphere/contents/430000000000000B/`) and reboot.
2. Join the same Wi-Fi as the computer. `switch-use discover` finds it.

## What an agent sees

The Switch has no accessibility tree, so `elements` is the stand-in: a screenshot, read with OCR,
every line of text with the point to touch.

```text
$ switch-use elements
screenshot /tmp/switch-screen-1791731864357.jpg
[0] 集合啦！动物森友会  (425,64)
[1] Ver.1.2.1  (341,104)
[4] 软件信息  (240,192)
[9] 数据管理  (240,351)
[17] 返回  (962,684)
[18] 确定  (1085,685)
```

```sh
switch-use tap 数据管理                       # touch the text that matches
switch-use --observe press B see:关闭软件      # act, wait for a screen, print what it shows
switch-use wait 继续 --timeout 8000           # block until text appears (or --absent)
```

`--observe` waits until both the text and the pixels stop changing, so a loading spinner is not
mistaken for a finished screen. Known screens get a `hint:` line (for example the
controller-pairing screen after wake).

## All commands

```sh
switch-use discover                 # find the Switch on the LAN, remember it
switch-use status [--json]          # sys-botbase version, battery, running game
switch-use elements [--json]        # on-screen text + touch points + screenshot path
switch-use tap <text> [--nth N]     # touch a label; exits 1 if the screen did not react
switch-use wait <text> [--absent]   # until text appears / disappears
switch-use screen -o now.jpg        # 1280x720 JPEG
switch-use press 'DDOWN*3' A see:设置 gone:加载中 wait:500
switch-use hold ZL ZR --ms 800
switch-use stick L up --ms 400      # or: stick R 0.5 -1
switch-use touch 640 360 [--hold 500]
switch-use swipe 200 600 1000 600
switch-use type "Hello!" --enter    # ASCII through a virtual USB keyboard
switch-use home                     # HOME menu; does not toggle back into the game
switch-use display off|on           # backlight off during long runs
switch-use raw <sys-botbase command> [--reply]
switch-use upgrade [--check]
```

Add `--observe` to any action to get the settled screen back. [SKILL.md](SKILL.md) has the agent
workflow and the gotchas measured on a real console (dropped inputs during transitions, HOME being
a toggle, the close-software dialog, typing limits).

## FAQ

**Does it need a modded Switch?** Yes: Atmosphère with the sys-botbase sysmodule. It does not work
on stock firmware.

**Does OCR work on Linux?** Not yet. OCR uses macOS Vision; on Linux every command except
`elements`, `tap`, `wait`, `see:`/`gone:` and `--observe` works, and you read the screenshot.

**Docked or handheld?** Both. Buttons, sticks and screenshots work either way, and sys-botbase's
virtual touchscreen worked docked in testing too. On the HOME menu the first touch selects a tile;
touch again to open it.

**How is this different from SysBot.NET?** SysBot.NET is a .NET framework for Pokémon trade bots
built on sys-botbase. switch-use is a general CLI for driving any screen, made for AI agents:
OCR, text waits, settled-screen observation.

**Is it fast?** A screenshot takes 0.2–1.5 s depending on how busy the screen is (sys-botbase
sends JPEGs as hex). OCR adds about 0.5 s; an action with `--observe` returns in ~2 s.

## Safety

Use emuMMC with Nintendo's servers blocked (Atmosphère `hosts/emummc.txt`) and PRODINFO blanked.
switch-use only talks to sys-botbase on your LAN; it never contacts Nintendo. The agent skill asks
before interrupting a running game, changing settings, deleting data or buying anything.

## License

MIT OR Apache-2.0
