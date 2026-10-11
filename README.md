# switch-use

Drive a Nintendo Switch running Atmosphère from the command line or an AI agent: buttons, sticks,
touch, typing, screenshots and on-screen text (OCR), over the LAN.

A single ~0.9 MB Rust binary that speaks [sys-botbase](https://github.com/olliz0r/sys-botbase)'s
TCP protocol. Part of the `*-use` family (iphone-use, chrome-use, mail-use, …).

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/leeguooooo/switch-use/main/install.sh | sh
```

Intel Macs and other platforms: `cargo install --git https://github.com/leeguooooo/switch-use`.

On the Switch (Atmosphère, booted into emuMMC):

1. Copy [sys-botbase](https://github.com/olliz0r/sys-botbase/releases) to the SD card
   (`atmosphere/contents/430000000000000B/`) and reboot.
2. Join the same Wi-Fi as the computer.

## Use

```sh
switch-use discover                 # find the Switch on the LAN, remember it
switch-use status                   # sys-botbase version, battery, running game
switch-use elements                 # text on screen (macOS Vision OCR) with touch points
switch-use tap 设置                  # touch a label (handheld)
switch-use --observe press A see:继续   # act, wait for text, print the settled screen
switch-use screen -o now.jpg        # 1280x720 JPEG in ~0.3 s
switch-use press DDOWN*3 A wait:1500
switch-use stick L up --ms 400
switch-use touch 640 360
switch-use type "Hello!" --enter
switch-use home
```

See [SKILL.md](SKILL.md) for the agent workflow and the timing gotchas measured on a console.

## Safety

The console should be on emuMMC with Nintendo's servers blocked (Atmosphère `hosts/emummc.txt`)
and PRODINFO blanked. switch-use only talks to sys-botbase on your LAN.

## License

MIT OR Apache-2.0
