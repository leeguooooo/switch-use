//! switch-use: drive a homebrew Nintendo Switch over sys-botbase (TCP 6000).
//!
//! sys-botbase speaks a line protocol: one command per line, `\r\n` terminated.
//! Only query commands (getVersion, pixelPeek, charge, ...) answer, with a single
//! `\n`-terminated line. Input commands are executed on the console's main loop,
//! one after another, so we pace them client-side instead of firing a burst.

mod frame;
mod ocr;
mod upgrade;

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

const PORT: u16 = 6000;
const BUTTONS: &[&str] = &[
    "A", "B", "X", "Y", "L", "R", "ZL", "ZR", "PLUS", "MINUS", "LSTICK", "RSTICK", "HOME",
    "CAPTURE", "DUP", "DDOWN", "DLEFT", "DRIGHT",
];

#[derive(Parser)]
#[command(name = "switch-use", version, about = "Drive a homebrew Nintendo Switch over sys-botbase")]
struct Cli {
    /// Switch IP or host (default: $SWITCH_USE_HOST, then the saved config, then auto-discover)
    #[arg(long, global = true, env = "SWITCH_USE_HOST")]
    host: Option<String>,
    /// After an action, wait for the screen to settle and print its text (like `elements`)
    #[arg(long, global = true)]
    observe: bool,
    /// JSON output for `status`, `elements`, `wait`, `upgrade` and --observe
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Scan the local /24 for sys-botbase and save the host to the config
    Discover {
        /// Subnet prefix to scan, e.g. 192.168.50 (default: this Mac's primary subnet)
        #[arg(long)]
        subnet: Option<String>,
    },
    /// Connection, sys-botbase version, battery and the running title
    Status,
    /// Save a 1280x720 JPEG of the current screen and print its path
    Screen {
        /// Output file (default: switch-screen-<unix time>.jpg in the temp dir)
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Read the text on screen (macOS Vision OCR): index, text, center point to touch
    Elements {
        /// OCR an existing screenshot instead of the console
        #[arg(long)]
        from: Option<PathBuf>,
        /// Where to keep the screenshot (default: temp dir)
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Touch the on-screen text matching a label
    Tap {
        label: String,
        /// Which match when several are on screen, 0-based in reading order
        #[arg(long)]
        nth: Option<usize>,
        /// Hold instead of tap, ms
        #[arg(long)]
        hold: Option<u64>,
    },
    /// Wait until text appears on screen (or is gone with --absent)
    Wait {
        text: String,
        #[arg(long)]
        absent: bool,
        #[arg(long, default_value_t = 10000)]
        timeout: u64,
    },
    /// Click buttons in order. Tokens: A, B, DDOWN*3 (repeat), wait:500 (ms),
    /// see:设置 (wait until that text is on screen), gone:加载中 (until it is not)
    Press {
        #[arg(required = true)]
        keys: Vec<String>,
        /// How long each click is held, ms
        #[arg(long, default_value_t = 100)]
        hold: u64,
        /// Pause after each click, ms (menus drop inputs that arrive too fast)
        #[arg(long, default_value_t = 250)]
        gap: u64,
    },
    /// Hold one or more buttons down together for a duration
    Hold {
        #[arg(required = true)]
        keys: Vec<String>,
        #[arg(long, default_value_t = 1000)]
        ms: u64,
    },
    /// Push a stick: direction (up/down/left/right/center) or x y in -1.0..1.0
    Stick {
        /// L or R
        side: String,
        #[arg(required = true, num_args = 1..=2, allow_hyphen_values = true)]
        dir: Vec<String>,
        /// How long to hold before re-centering, ms (0 = leave it pushed)
        #[arg(long, default_value_t = 200)]
        ms: u64,
    },
    /// Tap the touchscreen, 0..1280 x 0..720
    Touch {
        x: u32,
        y: u32,
        /// Hold instead of tap, ms
        #[arg(long)]
        hold: Option<u64>,
    },
    /// Drag a finger through the given points: x1 y1 x2 y2 ...
    Swipe {
        #[arg(required = true, num_args = 4..)]
        points: Vec<u32>,
    },
    /// Type text through a virtual USB keyboard (works in the system keyboard)
    Type {
        text: String,
        /// Press Enter after the text
        #[arg(long)]
        enter: bool,
    },
    /// Go to the HOME menu
    Home,
    /// Turn the display backlight off (saves battery while automating) or on
    Display { state: String },
    /// Update switch-use to the latest release, and its skill
    Upgrade {
        /// Only report whether a newer release exists
        #[arg(long)]
        check: bool,
    },
    /// Send a raw sys-botbase command; prints the reply when --reply is given
    Raw {
        #[arg(required = true, num_args = 1..)]
        command: Vec<String>,
        #[arg(long)]
        reply: bool,
    },
}

struct Bot {
    conn: TcpStream,
    reader: BufReader<TcpStream>,
}

impl Bot {
    fn connect(host: &str) -> Result<Self> {
        let addr = resolve(host)?;
        let conn = TcpStream::connect_timeout(&addr, Duration::from_secs(3)).with_context(|| {
            format!("cannot reach sys-botbase at {addr} (is the Switch on, on Wi-Fi, and booted into Atmosphère?)")
        })?;
        conn.set_read_timeout(Some(Duration::from_secs(10)))?;
        conn.set_nodelay(true)?;
        let reader = BufReader::new(conn.try_clone()?);
        Ok(Bot { conn, reader })
    }

    fn send(&mut self, cmd: &str) -> Result<()> {
        self.conn.write_all(format!("{cmd}\r\n").as_bytes())?;
        Ok(())
    }

    fn query(&mut self, cmd: &str) -> Result<String> {
        self.send(cmd)?;
        let mut line = String::new();
        self.reader.read_line(&mut line).with_context(|| format!("no reply to `{cmd}`"))?;
        Ok(line.trim().to_string())
    }

    /// Input commands give no reply. A cheap query afterwards returns only once
    /// the console has executed everything before it, so callers know it landed.
    fn sync(&mut self) -> Result<()> {
        self.query("getVersion").map(|_| ())
    }

    fn click(&mut self, button: &str, hold: u64, gap: u64) -> Result<()> {
        self.send(&format!("press {button}"))?;
        sleep(Duration::from_millis(hold));
        self.send(&format!("release {button}"))?;
        self.sync()?;
        sleep(Duration::from_millis(gap));
        Ok(())
    }
}

fn resolve(host: &str) -> Result<SocketAddr> {
    let target = if host.contains(':') { host.to_string() } else { format!("{host}:{PORT}") };
    target.to_socket_addrs()?.next().ok_or_else(|| anyhow!("cannot resolve {host}"))
}

fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    base.join("switch-use").join("host")
}

fn saved_host() -> Option<String> {
    std::fs::read_to_string(config_path()).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn save_host(host: &str) -> Result<()> {
    let p = config_path();
    std::fs::create_dir_all(p.parent().unwrap())?;
    std::fs::write(&p, format!("{host}\n"))?;
    Ok(())
}

fn local_subnet() -> Result<String> {
    // The UDP "connect" picks the outbound interface without sending anything.
    let s = std::net::UdpSocket::bind("0.0.0.0:0")?;
    s.connect("1.1.1.1:80")?;
    let ip = s.local_addr()?.ip().to_string();
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() != 4 {
        bail!("no IPv4 address on the primary interface ({ip}); pass --subnet");
    }
    Ok(parts[..3].join("."))
}

fn discover(subnet: Option<String>) -> Result<String> {
    let subnet = match subnet {
        Some(s) => s,
        None => local_subnet()?,
    };
    let handles: Vec<_> = (1..=254)
        .map(|i| {
            let host = format!("{subnet}.{i}");
            std::thread::spawn(move || {
                let addr: SocketAddr = format!("{host}:{PORT}").parse().ok()?;
                let conn = TcpStream::connect_timeout(&addr, Duration::from_millis(800)).ok()?;
                conn.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
                let mut w = conn.try_clone().ok()?;
                w.write_all(b"getVersion\r\n").ok()?;
                let mut line = String::new();
                BufReader::new(conn).read_line(&mut line).ok()?;
                let v = line.trim().to_string();
                (!v.is_empty()).then_some((host, v))
            })
        })
        .collect();
    let found: Vec<(String, String)> = handles.into_iter().filter_map(|h| h.join().ok().flatten()).collect();
    match found.as_slice() {
        [] => bail!("no sys-botbase found on {subnet}.0/24 (Switch asleep, off Wi-Fi, or on another subnet?)"),
        [(host, v), rest @ ..] => {
            for (h, v) in rest {
                eprintln!("also found {h} (sys-botbase {v})");
            }
            save_host(host)?;
            eprintln!("found {host} (sys-botbase {v}); saved to {}", config_path().display());
            Ok(host.clone())
        }
    }
}

fn host_or_discover(cli_host: Option<String>) -> Result<String> {
    if let Some(h) = cli_host.or_else(saved_host) {
        return Ok(h);
    }
    eprintln!("no host configured, scanning the local network...");
    discover(None)
}

fn connect(cli_host: Option<String>) -> Result<Bot> {
    let explicit = cli_host.is_some();
    let host = host_or_discover(cli_host)?;
    match Bot::connect(&host) {
        Ok(b) => Ok(b),
        // A saved address can go stale after DHCP hands the Switch a new one.
        Err(e) if !explicit => {
            eprintln!("{e:#}; rescanning...");
            Bot::connect(&discover(None)?)
        }
        Err(e) => Err(e),
    }
}

fn button(name: &str) -> Result<String> {
    let up = name.to_ascii_uppercase();
    let alias = match up.as_str() {
        "UP" | "DU" => "DUP",
        "DOWN" | "DD" => "DDOWN",
        "LEFT" | "DL" => "DLEFT",
        "RIGHT" | "DR" => "DRIGHT",
        "+" | "START" => "PLUS",
        "-" | "SELECT" => "MINUS",
        "LS" => "LSTICK",
        "RS" => "RSTICK",
        other => other,
    };
    if BUTTONS.contains(&alias) {
        Ok(alias.to_string())
    } else {
        bail!("unknown button `{name}` (valid: {})", BUTTONS.join(" "))
    }
}

fn stick_value(v: f64) -> i32 {
    (v.clamp(-1.0, 1.0) * 32767.0).round() as i32
}

fn stick_xy(dir: &[String]) -> Result<(i32, i32)> {
    if dir.len() == 2 {
        let x: f64 = dir[0].parse().context("x must be a number in -1.0..1.0")?;
        let y: f64 = dir[1].parse().context("y must be a number in -1.0..1.0")?;
        return Ok((stick_value(x), stick_value(y)));
    }
    Ok(match dir[0].to_ascii_lowercase().as_str() {
        "up" => (0, 32767),
        "down" => (0, -32768),
        "left" => (-32768, 0),
        "right" => (32767, 0),
        "center" | "centre" | "neutral" => (0, 0),
        d => bail!("unknown direction `{d}` (up/down/left/right/center, or x y)"),
    })
}

/// USB HID usage id and whether Shift is needed, for a US keyboard layout.
fn hid_key(c: char) -> Option<(u8, bool)> {
    let lower = "abcdefghijklmnopqrstuvwxyz";
    if let Some(i) = lower.find(c) {
        return Some((4 + i as u8, false));
    }
    if let Some(i) = lower.find(c.to_ascii_lowercase()) {
        return Some((4 + i as u8, true));
    }
    let table: &[(char, u8, bool)] = &[
        ('1', 30, false), ('2', 31, false), ('3', 32, false), ('4', 33, false), ('5', 34, false),
        ('6', 35, false), ('7', 36, false), ('8', 37, false), ('9', 38, false), ('0', 39, false),
        ('!', 30, true), ('@', 31, true), ('#', 32, true), ('$', 33, true), ('%', 34, true),
        ('^', 35, true), ('&', 36, true), ('*', 37, true), ('(', 38, true), (')', 39, true),
        ('\n', 40, false), (' ', 44, false), ('-', 45, false), ('_', 45, true), ('=', 46, false),
        ('+', 46, true), ('[', 47, false), ('{', 47, true), (']', 48, false), ('}', 48, true),
        ('\\', 49, false), ('|', 49, true), (';', 51, false), (':', 51, true), ('\'', 52, false),
        ('"', 52, true), ('`', 53, false), ('~', 53, true), (',', 54, false), ('<', 54, true),
        ('.', 55, false), ('>', 55, true), ('/', 56, false), ('?', 56, true),
    ];
    table.iter().find(|(ch, _, _)| *ch == c).map(|&(_, k, s)| (k, s))
}

fn type_text(bot: &mut Bot, text: &str, enter: bool) -> Result<()> {
    // The system keyboard ignores the Shift modifier flag (keyMod); it only sees
    // Shift when the LeftShift key itself is down, so shifted characters go out
    // as `keyMulti 225 <key>`. Unshifted runs batch into one sequential `key`.
    const LEFT_SHIFT: u8 = 225;
    let mut keys = Vec::new();
    for c in text.chars() {
        let (k, shift) = hid_key(c).ok_or_else(|| anyhow!("cannot type `{c}`: only ASCII is supported"))?;
        keys.push((k, shift));
    }
    if enter {
        keys.push((40, false));
    }
    let mut i = 0;
    while i < keys.len() {
        let (k, shift) = keys[i];
        let count = if shift {
            bot.send(&format!("keyMulti {LEFT_SHIFT} {k}"))?;
            1
        } else {
            let run: Vec<String> =
                keys[i..].iter().take_while(|(_, s)| !s).take(32).map(|(k, _)| k.to_string()).collect();
            bot.send(&format!("key {}", run.join(" ")))?;
            run.len()
        };
        bot.sync()?;
        // Typing runs on its own thread on the console; let it finish first.
        sleep(Duration::from_millis(50 * count as u64 + 150));
        i += count;
    }
    Ok(())
}

fn timestamp() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
}

fn grab(bot: &mut Bot) -> Result<Vec<u8>> {
    decode_hex(&bot.query("pixelPeek")?).context("pixelPeek returned no image")
}

fn save_shot(jpg: &[u8], out: Option<PathBuf>) -> Result<PathBuf> {
    let path = out.unwrap_or_else(|| std::env::temp_dir().join(format!("switch-screen-{}.jpg", timestamp())));
    std::fs::write(&path, jpg).with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}

/// Screens with a known way out, recognised by their text.
fn hint(items: &[ocr::Text]) -> Option<&'static str> {
    let all: String = items.iter().map(|t| t.text.as_str()).collect();
    // The controller-pairing applet shows up after wake or when the virtual controller dropped.
    if all.contains("手柄上按下") || all.contains("手把上按下") || all.contains("Press L + R") {
        return Some("controller pairing screen: `switch-use hold L R`, then `switch-use press A`");
    }
    None
}

fn print_elements(path: &std::path::Path, items: &[ocr::Text], json: bool, settled: bool) {
    if json {
        let list: Vec<_> = items.iter().enumerate().map(|(i, t)| t.json(i)).collect();
        println!(
            "{}",
            serde_json::json!({ "screenshot": path, "elements": list, "hint": hint(items), "settled": settled })
        );
        return;
    }
    println!("screenshot {}", path.display());
    if !settled {
        println!("note: the screen was still changing (loading or animation); read again or look at the screenshot");
    }
    if let Some(h) = hint(items) {
        println!("hint: {h}");
    }
    if items.is_empty() {
        println!("(no text on screen; look at the screenshot)");
    }
    for (i, t) in items.iter().enumerate() {
        let (x, y) = t.center();
        let low = if t.conf < 0.5 { "  ?" } else { "" };
        println!("[{i}] {}  ({x},{y}){low}", t.text);
    }
}

/// OCR `jpg` while the next screenshot downloads, so a polling loop costs one
/// round trip per read instead of two.
fn read_and_grab(bot: &mut Bot, jpg: &[u8]) -> Result<(Vec<ocr::Text>, Vec<u8>)> {
    std::thread::scope(|s| {
        let next = s.spawn(|| grab(bot));
        let items = ocr::read(jpg);
        let next = next.join().map_err(|_| anyhow!("screenshot thread panicked"))?;
        Ok((items?, next?))
    })
}

/// Poll the screen until `text` is present (or absent). Returns the match.
fn wait_text(bot: &mut Bot, text: &str, absent: bool, timeout: u64) -> Result<Option<ocr::Text>> {
    let t = Instant::now();
    let mut jpg = grab(bot)?;
    loop {
        let (items, next) = read_and_grab(bot, &jpg)?;
        let hit = ocr::find(&items, text).first().map(|(_, t)| (*t).clone());
        if hit.is_some() != absent {
            return Ok(hit);
        }
        if t.elapsed() >= Duration::from_millis(timeout) {
            let seen: Vec<_> = items.iter().map(|t| t.text.as_str()).collect();
            bail!(
                "`{text}` still {} after {timeout} ms; on screen: {}",
                if absent { "on screen" } else { "not on screen" },
                if seen.is_empty() { "(no text)".into() } else { seen.join(" | ") }
            );
        }
        jpg = next;
    }
}

/// The text that decides whether the screen has settled. Low-confidence reads
/// (box art, pixel fonts) change between identical frames, so they don't count.
fn settled_text(items: &[ocr::Text]) -> Vec<&str> {
    items.iter().filter(|t| t.conf >= 0.5).map(|t| t.text.as_str()).collect()
}

/// Wait until two reads in a row show the same text on a still screen (max ~4 s),
/// then print the last. Pixels matter too: a loading spinner carries no text.
fn observe(bot: &mut Bot, json: bool) -> Result<()> {
    let t = Instant::now();
    sleep(Duration::from_millis(150));
    let mut jpg = grab(bot)?;
    let mut prev: Option<(Vec<ocr::Text>, Vec<u8>)> = None;
    loop {
        let (items, next) = read_and_grab(bot, &jpg)?;
        let thumb = frame::thumb(&jpg)?;
        let same = prev
            .as_ref()
            .is_some_and(|(p, pt)| settled_text(p) == settled_text(&items) && frame::still(pt, &thumb));
        if same || t.elapsed() >= Duration::from_secs(4) {
            print_elements(&save_shot(&jpg, None)?, &items, json, same);
            return Ok(());
        }
        prev = Some((items, thumb));
        jpg = next;
    }
}

/// After a touch, check the screen reacted. Docked there is no touchscreen and
/// sys-botbase cannot tell, so a touch that changes nothing is reported.
fn check_touch(bot: &mut Bot, before: &[u8]) -> Result<()> {
    let before = frame::thumb(before)?;
    sleep(Duration::from_millis(250));
    let after = frame::thumb(&grab(bot)?)?;
    if frame::still(&before, &after) {
        bail!(
            "the touch changed nothing on screen (nothing there reacts to touch); look at the screenshot, \
             or navigate with the D-pad and A"
        );
    }
    Ok(())
}

/// The HOME menu shows the clock in the top-right corner; games and applets do not.
fn at_home(items: &[ocr::Text]) -> bool {
    items.iter().any(|t| {
        let s = t.text.trim();
        t.x > 850 && t.y < 110 && s.len() <= 5 && s.contains(':') && s.chars().all(|c| c.is_ascii_digit() || c == ':')
    })
}

/// HOME is a toggle: pressed on the HOME menu it resumes the running game. Press,
/// then confirm with OCR, and press once more if it toggled the wrong way.
fn go_home(bot: &mut Bot) -> Result<()> {
    // Already there: pressing would resume the game, if only for a moment.
    if at_home(&ocr::read(&grab(bot)?)?) {
        return Ok(());
    }
    for attempt in 0..2 {
        bot.click("HOME", 100, 0)?;
        let t = Instant::now();
        while t.elapsed() < Duration::from_millis(2500) {
            sleep(Duration::from_millis(300));
            if at_home(&ocr::read(&grab(bot)?)?) {
                // The fade-in drops input for a moment longer.
                sleep(Duration::from_millis(400));
                return Ok(());
            }
        }
        if attempt == 0 {
            eprintln!("not on the HOME menu after pressing HOME (it toggles back to a running game); pressing again");
        }
    }
    bail!("could not reach the HOME menu; look at `switch-use screen`")
}

fn not_found(items: &[ocr::Text], label: &str) -> anyhow::Error {
    let near: Vec<_> = ocr::near(items, label).iter().map(|t| format!("`{}`", t.text)).collect();
    if near.is_empty() {
        anyhow!("`{label}` is not on screen (run `switch-use elements`)")
    } else {
        anyhow!("`{label}` is not on screen; did you mean {}?", near.join(", "))
    }
}

fn run(cli: Cli) -> Result<()> {
    let acted = matches!(
        cli.cmd,
        Cmd::Press { .. } | Cmd::Hold { .. } | Cmd::Stick { .. } | Cmd::Touch { .. } | Cmd::Swipe { .. }
            | Cmd::Type { .. } | Cmd::Home | Cmd::Tap { .. }
    );
    let (observe_after, json, host) = (cli.observe, cli.json, cli.host.clone());
    run_cmd(cli)?;
    if observe_after && acted {
        observe(&mut connect(host)?, json)?;
    }
    Ok(())
}

fn run_cmd(cli: Cli) -> Result<()> {
    let json = cli.json;
    match cli.cmd {
        Cmd::Discover { subnet } => {
            println!("{}", discover(subnet)?);
        }
        Cmd::Status => {
            let host = host_or_discover(cli.host.clone())?;
            let mut bot = connect(Some(host.clone()))?;
            let version = bot.query("getVersion")?;
            let battery = bot.query("charge")?;
            let title = bot.query("getTitleID")?;
            let name = if title.is_empty() { String::new() } else { bot.query("game name")? };
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "host": host, "sys_botbase": version,
                        "battery": battery.parse::<u32>().ok(),
                        "title_id": (!title.is_empty()).then_some(&title),
                        "title": (!name.is_empty()).then_some(&name),
                    })
                );
            } else {
                println!("host        {host}");
                println!("sys-botbase {version}");
                println!("battery     {battery}%");
                if title.is_empty() {
                    println!("running     (no game; HOME menu or a system applet)");
                } else {
                    println!("running     {name} [{title}]");
                }
            }
        }
        Cmd::Screen { out } => {
            let mut bot = connect(cli.host)?;
            let t = Instant::now();
            let jpg = grab(&mut bot)?;
            let path = save_shot(&jpg, out)?;
            eprintln!("{} bytes in {:.2}s", jpg.len(), t.elapsed().as_secs_f64());
            println!("{}", path.display());
        }
        Cmd::Elements { from, out } => {
            let t = Instant::now();
            let (path, jpg) = match from {
                Some(p) => {
                    let jpg = std::fs::read(&p).with_context(|| format!("read {}", p.display()))?;
                    (p, jpg)
                }
                None => {
                    let jpg = grab(&mut connect(cli.host)?)?;
                    (save_shot(&jpg, out)?, jpg)
                }
            };
            let items = ocr::read(&jpg)?;
            eprintln!("{} texts in {:.2}s", items.len(), t.elapsed().as_secs_f64());
            print_elements(&path, &items, json, true);
        }
        Cmd::Tap { label, nth, hold } => {
            let mut bot = connect(cli.host)?;
            let before = grab(&mut bot)?;
            let items = ocr::read(&before)?;
            let hits = ocr::find(&items, &label);
            let (i, t) = match (hits.as_slice(), nth) {
                ([], _) => return Err(not_found(&items, &label)),
                ([one], None) => *one,
                (many, Some(n)) => *many.get(n).ok_or_else(|| anyhow!("--nth {n}: only {} matches", many.len()))?,
                (many, None) => {
                    let list: Vec<_> = many
                        .iter()
                        .enumerate()
                        .map(|(n, (_, t))| format!("--nth {n}: `{}` {:?}", t.text, t.center()))
                        .collect();
                    bail!("`{label}` matches {} texts; pick one:\n  {}", many.len(), list.join("\n  "));
                }
            };
            let (x, y) = t.center();
            match hold {
                Some(ms) => bot.send(&format!("touchHold {x} {y} {}", ms.max(15)))?,
                None => bot.send(&format!("touch {x} {y}"))?,
            }
            bot.sync()?;
            sleep(Duration::from_millis(hold.unwrap_or(0) + 150));
            eprintln!("touched [{i}] `{}` at ({x},{y})", t.text);
            check_touch(&mut bot, &before)?;
        }
        Cmd::Wait { text, absent, timeout } => {
            let mut bot = connect(cli.host)?;
            let t = Instant::now();
            let hit = wait_text(&mut bot, &text, absent, timeout)?;
            let ms = t.elapsed().as_millis();
            match (hit, json) {
                (Some(h), true) => println!("{}", serde_json::json!({ "found": h.json(0), "ms": ms })),
                (Some(h), false) => println!("found `{}` at {:?} after {ms} ms", h.text, h.center()),
                (None, true) => println!("{}", serde_json::json!({ "gone": text, "ms": ms })),
                (None, false) => println!("`{text}` gone after {ms} ms"),
            }
        }
        Cmd::Press { keys, hold, gap } => {
            let mut bot = connect(cli.host)?;
            for tok in &keys {
                if let Some(ms) = tok.strip_prefix("wait:") {
                    sleep(Duration::from_millis(ms.parse().context("wait:<ms>")?));
                    continue;
                }
                if let Some(text) = tok.strip_prefix("see:") {
                    wait_text(&mut bot, text, false, 10000)?;
                    continue;
                }
                if let Some(text) = tok.strip_prefix("gone:") {
                    wait_text(&mut bot, text, true, 10000)?;
                    continue;
                }
                let (name, times) = match tok.split_once('*') {
                    Some((n, t)) => (n, t.parse::<u32>().context("repeat count after *")?),
                    None => (tok.as_str(), 1),
                };
                let b = button(name)?;
                for _ in 0..times {
                    bot.click(&b, hold, gap)?;
                }
            }
        }
        Cmd::Hold { keys, ms } => {
            let mut bot = connect(cli.host)?;
            let bs: Vec<String> = keys.iter().map(|k| button(k)).collect::<Result<_>>()?;
            for b in &bs {
                bot.send(&format!("press {b}"))?;
            }
            sleep(Duration::from_millis(ms));
            for b in &bs {
                bot.send(&format!("release {b}"))?;
            }
            bot.sync()?;
        }
        Cmd::Stick { side, dir, ms } => {
            let side = match side.to_ascii_uppercase().as_str() {
                "L" | "LEFT" => "LEFT",
                "R" | "RIGHT" => "RIGHT",
                s => bail!("stick must be L or R, got {s}"),
            };
            let (x, y) = stick_xy(&dir)?;
            let mut bot = connect(cli.host)?;
            bot.send(&format!("setStick {side} {x} {y}"))?;
            if ms > 0 {
                sleep(Duration::from_millis(ms));
                bot.send(&format!("setStick {side} 0 0"))?;
            }
            bot.sync()?;
        }
        Cmd::Touch { x, y, hold } => {
            if x > 1280 || y > 720 {
                bail!("touch is 0..1280 x 0..720");
            }
            let mut bot = connect(cli.host)?;
            let before = grab(&mut bot)?;
            match hold {
                Some(ms) => bot.send(&format!("touchHold {x} {y} {}", ms.max(15)))?,
                None => bot.send(&format!("touch {x} {y}"))?,
            }
            bot.sync()?;
            sleep(Duration::from_millis(hold.unwrap_or(0) + 150));
            check_touch(&mut bot, &before)?;
        }
        Cmd::Swipe { points } => {
            if points.len() % 2 != 0 {
                bail!("swipe takes x y pairs");
            }
            let mut bot = connect(cli.host)?;
            let args: Vec<String> = points.iter().map(u32::to_string).collect();
            bot.send(&format!("touchDraw {}", args.join(" ")))?;
            bot.sync()?;
            sleep(Duration::from_millis(40 * points.len() as u64 + 150));
        }
        Cmd::Type { text, enter } => {
            let mut bot = connect(cli.host)?;
            type_text(&mut bot, &text, enter)?;
        }
        Cmd::Home => {
            let mut bot = connect(cli.host)?;
            if cfg!(target_os = "macos") {
                go_home(&mut bot)?;
            } else {
                // No OCR to confirm with. The fade-in drops input for about a second.
                bot.click("HOME", 100, 1200)?;
            }
        }
        Cmd::Display { state } => {
            let mut bot = connect(cli.host)?;
            match state.to_ascii_lowercase().as_str() {
                "off" => bot.send("screenOff")?,
                "on" => bot.send("screenOn")?,
                s => bail!("display takes on or off, got {s}"),
            }
            bot.sync()?;
        }
        Cmd::Upgrade { .. } => unreachable!("handled in main"),
        Cmd::Raw { command, reply } => {
            let mut bot = connect(cli.host)?;
            let line = command.join(" ");
            if reply {
                println!("{}", bot.query(&line)?);
            } else {
                bot.send(&line)?;
                bot.sync()?;
            }
        }
    }
    Ok(())
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.is_empty() || s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

fn main() {
    let cli = Cli::parse();
    if let Cmd::Upgrade { check } = cli.cmd {
        std::process::exit(upgrade::run(check, cli.json));
    }
    upgrade::notice();
    if let Err(e) = run(cli) {
        eprintln!("switch-use: {e:#}");
        std::process::exit(1);
    }
}
