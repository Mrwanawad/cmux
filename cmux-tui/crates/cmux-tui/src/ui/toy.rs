//! Toys: Clawd, the Claude mascot, living in the chrome.
//!
//! The sprite is a small shaded Clawd, 8x6 square pixels (half a terminal
//! cell each, `▀`/`▄`): a lit top face, a shaded side face, a dark underside,
//! near legs lighter than far ones, so it reads as a block in 3/4 view. It
//! turns to face front, to its side and away (a spin), squashes when it lands
//! and swings its arms when it walks.
//!
//! Every Clawd wears a random outfit (a hat, a scarf or a tie, sometimes
//! shades) in a muted cloth color, and changes it mid-spin after a nap.
//!
//! A Clawd holds a tool that matches what its agent is doing, read from the
//! agent's own screen (Claude Code prints `⏺ Read(...)`, `⏺ Bash(...)`): a
//! magnifier for reading and searching, a pen for edits, a hard hat and
//! hammer for builds, a bug net for tests, a rocket for pushes and deploys,
//! a cloud for web fetches, a suit and briefcase for subagents, a beret and
//! brush for skills, a wrench for any other command. It uses the tool, walks,
//! looks around, sits, stretches, dances, spins and hops; it waves when the
//! agent needs the person and when it meets another Clawd, and when the agent
//! finishes it cheers, then naps a few seconds.
//!
//! With the sidebar open, Clawds live on the floor of the workspace rail's
//! empty space. Every pane that
//! hosts an agent also gets a Clawd in the deck above its tab bar; it stands
//! on the tab bar's free cells and visits the Clawd of a neighbor pane.
//!
//! In a Sixel host (Windows Terminal) the Clawds are drawn instead as real-pixel
//! 3D voxel models by `toy3d`: `pose3` turns each action into a 3D pose and
//! each toy area becomes one image, written after the cell frame.

use cmux_tui_core::{AgentStatus, Rect, SurfaceId};
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Style};
use std::time::{Duration, Instant};

/// Repaint cadence while a toy is on screen.
pub(crate) const FRAME: Duration = Duration::from_millis(80);

/// Never crowd the rail with more than this many toys.
const MAX_TOYS: usize = 7;
/// A header toy's world is the pane's deck rows plus its tab bar row, in
/// pixels (two per row).
const HEADER_PX: i32 = 2 * (crate::app::TOY_DECK_ROWS as i32 + 1);
/// Gravity in pixels per second squared.
const GRAVITY: f32 = 60.0;
/// Walking speed and a visitor's brisker pace, in sprite units per second.
const WALK: f32 = 5.0;
const VISIT: f32 = 14.0;
/// A Clawd naps this long (seconds) when it appears idle or turns idle.
const NAP: (f32, f32) = (3.0, 5.0);

/// The sprite in units (a unit is `scale` pixels square): width with the
/// arms, and full height: a hat row, four body rows and the legs.
const W: i32 = 8;
const H: i32 = 6;
/// Room beside the body for the tool in hand.
const PROP: i32 = 4;

/// Claude orange in four lights (top face, front face, side face,
/// underside) and the eyes.
const LIT: Color = Color::Rgb(0xF4, 0xA5, 0x86);
const ORANGE: Color = Color::Rgb(0xD9, 0x77, 0x57);
const SIDE: Color = Color::Rgb(0xB0, 0x5A, 0x3E);
const UNDER: Color = Color::Rgb(0x86, 0x40, 0x2B);
const INK: Color = Color::Rgb(0x14, 0x11, 0x10);
const SKY: Color = Color::Rgb(0x9C, 0xC8, 0xF0);

/// Outfit colors: muted cloth, small accents, and fixed trims.
const CLOTH: [Color; 7] = [
    Color::Rgb(0x24, 0x34, 0x5C), // navy
    Color::Rgb(0x80, 0x24, 0x30), // burgundy
    Color::Rgb(0x23, 0x5A, 0x3E), // bottle green
    Color::Rgb(0xC1, 0x9A, 0x6B), // camel
    Color::Rgb(0xB9, 0xB3, 0xA7), // stone
    Color::Rgb(0x3A, 0x3C, 0x44), // charcoal
    Color::Rgb(0x3D, 0x6F, 0xB0), // oxford blue
];
const ACCENT: [Color; 4] = [
    Color::Rgb(0xF1, 0xEA, 0xD8), // cream
    Color::Rgb(0xC8, 0x3A, 0x3A), // red
    Color::Rgb(0xE9, 0xB4, 0x4C), // gold
    Color::Rgb(0x9C, 0xC8, 0xF0), // sky
];
const CHARCOAL: Color = Color::Rgb(0x26, 0x26, 0x2C);
const GOLD: Color = Color::Rgb(0xE9, 0xB4, 0x4C);
const JEWEL: Color = Color::Rgb(0xC0, 0x39, 0x2B);
const WHITE: Color = Color::Rgb(0xF4, 0xF4, 0xF4);

fn darker(c: Color) -> Color {
    match c {
        Color::Rgb(r, g, b) => {
            let d = |v: u8| (u16::from(v) * 2 / 3) as u8;
            Color::Rgb(d(r), d(g), d(b))
        }
        other => other,
    }
}

fn lighter(c: Color) -> Color {
    match c {
        Color::Rgb(r, g, b) => {
            let l = |v: u8| v + (255 - v) / 3;
            Color::Rgb(l(r), l(g), l(b))
        }
        other => other,
    }
}

/// What a Clawd wears on its head.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Head {
    Bare,
    Fedora,
    Cap,
    Beanie,
    Headphones,
    Crown,
    Headband,
}

/// What a Clawd wears around its middle.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Neck {
    Bare,
    Scarf,
    Bowtie,
    Chain,
}

/// A Clawd's outfit: picked at random, changed after a nap.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Outfit {
    head: Head,
    neck: Neck,
    shades: bool,
    main: Color,
    accent: Color,
}

impl Outfit {
    const PLAIN: Outfit = Outfit {
        head: Head::Bare,
        neck: Neck::Bare,
        shades: false,
        main: CLOTH[0],
        accent: ACCENT[0],
    };
}

/// The tool palette: one letter per color in the prop art.
fn ink(ch: char) -> Option<Color> {
    let rgb = |r, g, b| Some(Color::Rgb(r, g, b));
    match ch {
        'G' => rgb(0xE9, 0xB4, 0x4C), // brass
        'L' => rgb(0xA8, 0xD8, 0xF0), // lens
        'w' => rgb(0xFF, 0xFF, 0xFF), // glint
        'B' => rgb(0x8A, 0x5A, 0x34), // wood
        'P' => rgb(0x45, 0x74, 0xDB), // pen
        'p' => rgb(0x2A, 0x48, 0x94), // pen cap
        'K' => rgb(0x3A, 0x3A, 0x40), // graphite
        'S' => rgb(0xA4, 0xAC, 0xB6), // steel
        's' => rgb(0x6E, 0x76, 0x80), // steel shade
        'Y' => rgb(0xF2, 0xC2, 0x30), // hard hat
        'y' => rgb(0xC9, 0x96, 0x1A), // hard hat brim
        'W' => rgb(0xEC, 0xEC, 0xEC), // white
        'M' => rgb(0x9C, 0xA3, 0xAF), // net mesh
        'g' => rgb(0x5D, 0xBB, 0x63), // bug
        'R' => rgb(0xE0, 0x4E, 0x4E), // red
        'F' => rgb(0xF5, 0x9E, 0x0B), // flame
        'f' => rgb(0xFD, 0xE0, 0x47), // flame tip
        'C' => rgb(0xD6, 0xEC, 0xFA), // cloud
        'A' => rgb(0x3F, 0x8F, 0xD8), // arrow
        'D' => rgb(0x2A, 0x2A, 0x33), // suit
        'T' => rgb(0xC0, 0x39, 0x2B), // tie
        'b' => rgb(0x8B, 0x5A, 0x2B), // briefcase
        'V' => rgb(0x2F, 0x2A, 0x3A), // beret
        'u' => rgb(0x9B, 0x6C, 0xF6), // paint
        _ => None,
    }
}

/// What a Clawd holds: the agent's current tool.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Tool {
    /// Read, search, grep, glob: a magnifier.
    Read,
    /// Edit, update, write: a pen.
    Edit,
    /// A build or install command: a hard hat and a hammer.
    Build,
    /// A test run: a bug net.
    Test,
    /// A push, deploy, release: a rocket.
    Ship,
    /// A web fetch or search: a cloud.
    Web,
    /// A subagent: a suit and a briefcase.
    Delegate,
    /// A skill: a beret and a brush.
    Skill,
    /// Any other command or tool: a wrench.
    Shell,
}

impl Tool {
    /// The tool for a Claude Code tool line's name and arguments.
    fn from_call(name: &str, args: &str) -> Tool {
        match name {
            "Read" | "Search" | "Grep" | "Glob" | "List" | "LS" | "Find" => Tool::Read,
            "Update" | "Edit" | "Write" | "MultiEdit" | "NotebookEdit" | "Create" => Tool::Edit,
            "Fetch" | "WebFetch" | "Web Search" | "WebSearch" => Tool::Web,
            "Task" | "Agent" => Tool::Delegate,
            "Skill" => Tool::Skill,
            "Bash" | "Shell" | "Run" | "PowerShell" => {
                let args = args.to_lowercase();
                let has = |words: &[&str]| words.iter().any(|word| args.contains(word));
                if has(&["test", "pytest", "jest", "vitest", "nextest", "spec"]) {
                    Tool::Test
                } else if has(&[
                    "git push",
                    "deploy",
                    "vercel",
                    "publish",
                    "gh release",
                    "gh pr create",
                ]) {
                    Tool::Ship
                } else if has(&[
                    "build", "make", "install", "cargo", "npm", "pnpm", "yarn", "pip", "docker",
                    "compile", "uv ",
                ]) {
                    Tool::Build
                } else {
                    Tool::Shell
                }
            }
            _ => Tool::Shell,
        }
    }
}

/// The tool an agent is using, read from its screen: the last Claude Code
/// bullet line (`⏺ Name(args)` or `● Name(args)`). A bullet that is prose,
/// not a call, means the agent is thinking or answering: no tool.
fn detect_tool(buf: &Buffer, body: Rect) -> Option<Tool> {
    let screen = *buf.area();
    let x0 = body.x.max(screen.x);
    let x1 = (body.x + body.width).min(screen.x + screen.width);
    let y0 = body.y.max(screen.y);
    let y1 = (body.y + body.height).min(screen.y + screen.height);
    let mut line = String::new();
    for y in (y0..y1).rev() {
        line.clear();
        for x in x0..x1 {
            line.push_str(buf[(x, y)].symbol());
        }
        let text = line.trim_start();
        let Some(rest) = text.strip_prefix('⏺').or_else(|| text.strip_prefix('●')) else {
            continue;
        };
        let rest = rest.trim_start();
        let name_len = rest.find(|c: char| !(c.is_ascii_alphabetic() || c == ' ')).unwrap_or(0);
        let name = rest[..name_len].trim_end();
        if name.is_empty() || !rest[name_len..].starts_with('(') {
            return None;
        }
        return Some(Tool::from_call(name, &rest[name_len + 1..]));
    }
    None
}

/// What a Clawd is doing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Action {
    /// Facing front, blinking now and then.
    Idle,
    /// Walking toward `target`, legs stepping with every unit it moves.
    Walk,
    /// Looking the way it faces.
    Look,
    /// Sitting low, legs folded.
    Sit,
    /// Up tall with its arms raised.
    Stretch,
    /// A little jump.
    Hop,
    /// Squashed for a moment after a hop lands.
    Land,
    /// Using the tool in its hand.
    Work,
    /// Waving: the agent needs the person, or a friend came by.
    Wave,
    /// Bobbing on the beat, arms up and down, turning side to side.
    Dance,
    /// Turning on the spot: side, front, other side, back.
    Spin,
    /// Facing front, arms up, bouncing: the agent finished.
    Cheer,
}

/// The surface id of the rail's napping Clawd when no agent works.
const NAPPER: SurfaceId = u64::MAX;

/// Where a header toy lives: the free cells of a pane's header row, the deck
/// rows above the pane, and the pane's terminal (where the tool is read).
#[derive(Clone, Copy, Debug)]
pub(crate) struct HeaderStrip {
    /// The free cells of the tab bar row: the toys' floor.
    pub rect: Rect,
    /// The deck rows above the tab bar, as wide as the pane.
    pub deck: Rect,
    /// The pane's terminal rows below the tab bar.
    pub body: Rect,
    pub surface: SurfaceId,
    pub status: AgentStatus,
}

/// How an agent is doing, as its Clawd shows it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mood {
    Working(Option<Tool>),
    Blocked,
    Idle,
}

/// One Clawd the world should hold: id, lane, floor range and mood.
#[derive(Clone, Copy, Debug)]
struct Want {
    id: SurfaceId,
    lane: u16,
    lo: f32,
    hi: f32,
    mood: Mood,
}

struct Body {
    id: SurfaceId,
    /// Header row (header world) or 0 (rail world).
    lane: u16,
    /// Current range for the left edge, in pixels; wider than `home` while
    /// a header Clawd visits a neighbor.
    lo: f32,
    hi: f32,
    home: (f32, f32),
    x: f32,
    /// Height above the floor, in pixels.
    y: f32,
    vx: f32,
    vy: f32,
    /// Facing right (walking, looking or holding its tool that way).
    right: bool,
    /// The agent waits at its prompt.
    idle: bool,
    /// The agent waits on the person.
    blocked: bool,
    tool: Option<Tool>,
    /// Seconds left of a nap.
    doze: f32,
    /// A nap to take once the current action (a cheer) ends.
    nap_after: f32,
    outfit: Outfit,
    /// Change outfit halfway through the next spin.
    restyle: bool,
    action: Action,
    act_t: f32,
    act_len: f32,
    /// Where a walk is headed (left edge, pixels).
    target: f32,
    bump_cd: f32,
    visit_cd: f32,
    /// A header Clawd walking over to a neighbor, and whether it is on its
    /// way back home.
    visiting: Option<(SurfaceId, bool)>,
    /// Offsets the blink clock so Clawds do not blink in step.
    phase: f32,
}

impl Body {
    fn grounded(&self) -> bool {
        self.y <= 0.0
    }

    fn asleep(&self) -> bool {
        self.doze > 0.0
    }

    fn start(&mut self, action: Action, len: f32) {
        self.action = action;
        self.act_t = 0.0;
        self.act_len = len;
    }
}

/// A toy world: the rail floor or the header rows. Lives on the App so
/// bodies keep their momentum from frame to frame.
pub(crate) struct ToyWorld {
    /// Pixels per sprite unit.
    scale: i32,
    /// Header Clawds visit their neighbors; rail Clawds stay put.
    visits: bool,
    toys: Vec<Body>,
    last: Option<Instant>,
    rng: u64,
    clock: f32,
    /// Sixel images for this frame (real-pixel 3D Clawds), and each toy's
    /// eased turn in degrees.
    pub(crate) jobs: Vec<super::toy3d::Job>,
    yaws: Vec<(SurfaceId, f32)>,
}

impl Default for ToyWorld {
    fn default() -> Self {
        Self {
            scale: 1,
            visits: false,
            toys: Vec::new(),
            last: None,
            rng: 0x2545_F491_4F6C_DD1D,
            clock: 0.0,
            jobs: Vec::new(),
            yaws: Vec::new(),
        }
    }
}

impl ToyWorld {
    /// Moves the last frame's time back by `by`, so a test's next frame
    /// steps the world as if that much time had passed.
    #[cfg(test)]
    pub(crate) fn rewind_for_test(&mut self, by: Duration) {
        self.last = self.last.map(|last| last - by);
    }

    /// The tool each agent's Clawd holds, for the rail to show the same.
    pub(crate) fn tools(&self) -> Vec<(SurfaceId, Tool)> {
        self.toys.iter().filter_map(|toy| toy.tool.map(|tool| (toy.id, tool))).collect()
    }

    fn rand(&mut self) -> f32 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.rand()
    }

    /// A random outfit: about one Clawd in six goes bare-headed, half wear
    /// nothing at the neck, one in four wears shades.
    fn new_outfit(&mut self) -> Outfit {
        const HEADS: [Head; 7] = [
            Head::Bare,
            Head::Fedora,
            Head::Cap,
            Head::Beanie,
            Head::Headphones,
            Head::Crown,
            Head::Headband,
        ];
        const NECKS: [Neck; 6] =
            [Neck::Bare, Neck::Bare, Neck::Bare, Neck::Scarf, Neck::Bowtie, Neck::Chain];
        let pick = |r: f32, n: usize| ((r * n as f32) as usize).min(n - 1);
        let (a, b, c, d, e) = (self.rand(), self.rand(), self.rand(), self.rand(), self.rand());
        Outfit {
            head: HEADS[pick(a, HEADS.len())],
            neck: NECKS[pick(b, NECKS.len())],
            shades: c < 0.25,
            main: CLOTH[pick(d, CLOTH.len())],
            accent: ACCENT[pick(e, ACCENT.len())],
        }
    }

    /// Seconds since the last step, capped so a stall does not teleport.
    fn tick(&mut self, now: Instant) -> f32 {
        let dt = self.last.map_or(0.0, |last| now.duration_since(last).as_secs_f32()).min(0.2);
        self.last = Some(now);
        self.clock += dt;
        dt
    }

    /// A body's footprint with its tool, in pixels.
    fn width(&self) -> f32 {
        ((W + PROP) * self.scale) as f32
    }

    fn height(&self) -> f32 {
        (H * self.scale) as f32
    }

    fn set_scale(&mut self, scale: i32) {
        if self.scale != scale {
            self.scale = scale;
            self.toys.clear();
        }
    }

    /// Make the world hold exactly `wanted`. A Clawd naps a few seconds
    /// when it appears idle (or is the napper) or its agent turns idle,
    /// then wakes with a stretch. It starts using a new tool right away.
    fn sync(&mut self, wanted: &[Want]) {
        self.toys.retain(|toy| wanted.iter().any(|w| w.id == toy.id));
        for want in wanted {
            let nap = self.range(NAP.0, NAP.1);
            let work = self.range(2.5, 4.5);
            let idle = want.mood == Mood::Idle;
            let blocked = want.mood == Mood::Blocked;
            let tool = match want.mood {
                Mood::Working(tool) => tool,
                _ => None,
            };
            let (lo, hi) = (want.lo, want.hi.max(want.lo));
            if let Some(toy) = self.toys.iter_mut().find(|toy| toy.id == want.id) {
                if idle && !toy.idle && !toy.asleep() {
                    // Done: a cheer, then the nap.
                    toy.nap_after = nap;
                    if toy.visiting.is_none() {
                        toy.start(Action::Cheer, 1.2);
                    }
                }
                if blocked && !toy.blocked && !toy.asleep() && toy.visiting.is_none() {
                    toy.start(Action::Wave, 2.0);
                }
                if tool.is_some() && tool != toy.tool && !toy.asleep() && toy.visiting.is_none() {
                    toy.start(Action::Work, work);
                }
                toy.idle = idle;
                toy.blocked = blocked;
                toy.tool = tool;
                toy.home = (lo, hi);
                if toy.lane != want.lane || toy.visiting.is_none() {
                    toy.lane = want.lane;
                    toy.lo = lo;
                    toy.hi = hi;
                    toy.x = toy.x.clamp(lo, hi);
                    toy.target = toy.target.clamp(lo, hi);
                }
                continue;
            }
            let x = self.free_spot(want.lane, lo, hi);
            let outfit = self.new_outfit();
            let toy = Body {
                id: want.id,
                lane: want.lane,
                lo,
                hi,
                home: (lo, hi),
                x,
                y: 0.0,
                vx: 0.0,
                vy: 0.0,
                right: self.rand() < 0.5,
                idle,
                blocked,
                tool,
                doze: if want.id == NAPPER || idle { nap } else { 0.0 },
                nap_after: 0.0,
                outfit,
                restyle: false,
                action: if tool.is_some() { Action::Work } else { Action::Idle },
                act_t: 0.0,
                act_len: if tool.is_some() { work } else { self.range(0.8, 2.0) },
                target: x,
                bump_cd: 0.0,
                visit_cd: self.range(6.0, 12.0),
                visiting: None,
                phase: self.range(0.0, 10.0),
            };
            self.toys.push(toy);
        }
    }

    /// A spot in [lo, hi] as far as possible from the toys already there.
    fn free_spot(&mut self, lane: u16, lo: f32, hi: f32) -> f32 {
        let mut best = (lo, f32::MIN);
        for _ in 0..12 {
            let x = self.range(lo, hi.max(lo)).round();
            let gap = self
                .toys
                .iter()
                .filter(|toy| toy.lane == lane)
                .map(|toy| (toy.x - x).abs())
                .fold(f32::MAX, f32::min);
            if gap > best.1 {
                best = (x, gap);
            }
        }
        best.0
    }

    /// The next thing a Clawd does once its current action ends.
    fn choose(&mut self, i: usize, ceiling: f32) {
        let (roll, a, b) = (self.rand(), self.rand(), self.rand());
        let scale = self.scale as f32;
        let toy = &mut self.toys[i];
        if toy.action == Action::Hop {
            toy.start(Action::Land, 0.16);
            return;
        }
        if let Some((_, true)) = toy.visiting {
            // Done visiting: walk home.
            toy.start(Action::Walk, 0.0);
            toy.target = ((toy.home.0 + toy.home.1) / 2.0).round();
            return;
        }
        if toy.nap_after > 0.0 {
            toy.doze = toy.nap_after;
            toy.nap_after = 0.0;
            toy.start(Action::Idle, 0.5);
            return;
        }
        if toy.blocked {
            toy.start(Action::Wave, 2.0);
            return;
        }
        if toy.restyle {
            toy.start(Action::Spin, 0.8);
            return;
        }
        let (lo, hi) = (toy.lo, toy.hi.max(toy.lo));
        let walk = |toy: &mut Body| {
            toy.start(Action::Walk, 0.0);
            let mut target = (lo + (hi - lo) * a).round();
            if (target - toy.x).abs() < 3.0 * scale {
                target = if toy.x - lo > hi - toy.x { lo } else { hi };
            }
            toy.target = target;
        };
        if toy.tool.is_some() {
            match roll {
                r if r < 0.66 => toy.start(Action::Work, 2.5 + 2.0 * a),
                r if r < 0.80 => walk(toy),
                r if r < 0.88 => toy.start(Action::Spin, 0.8),
                _ => {
                    toy.start(Action::Look, 1.0);
                    toy.right = b < 0.5;
                }
            }
            return;
        }
        match roll {
            r if r < 0.20 => toy.start(Action::Idle, 1.5 + 2.0 * a),
            r if r < 0.42 => walk(toy),
            r if r < 0.52 => {
                toy.start(Action::Look, 1.2);
                toy.right = b < 0.5;
            }
            r if r < 0.62 => toy.start(Action::Sit, 2.0 + 1.5 * a),
            r if r < 0.68 => toy.start(Action::Stretch, 0.9),
            r if r < 0.80 => toy.start(Action::Dance, 1.8 + 1.2 * a),
            r if r < 0.88 => {
                toy.start(Action::Spin, 0.8);
                toy.restyle = b < 0.4;
            }
            _ if ceiling >= 1.0 => {
                toy.start(Action::Hop, 0.0);
                toy.vy = (2.0 * GRAVITY * ceiling.min(4.0 * scale)).sqrt();
            }
            // No room overhead to hop: dance instead.
            _ => toy.start(Action::Dance, 1.8 + a),
        }
    }

    /// One physics step. `ceiling` is the highest a body may hop (pixels
    /// above the floor).
    fn step(&mut self, dt: f32, ceiling: f32) {
        let scale = self.scale as f32;
        for i in 0..self.toys.len() {
            let toy = &mut self.toys[i];
            toy.bump_cd -= dt;
            toy.visit_cd -= dt;
            if toy.doze > 0.0 {
                toy.doze -= dt;
                toy.vx = 0.0;
                toy.visiting = None;
                if toy.doze <= 0.0 {
                    // Up again with a stretch, then a spin into new clothes.
                    toy.doze = 0.0;
                    toy.start(Action::Stretch, 1.0);
                    toy.restyle = true;
                }
            } else {
                toy.act_t += dt;
                let done = match toy.action {
                    Action::Walk => (toy.target - toy.x).abs() < 0.5 || toy.act_t > 10.0,
                    Action::Hop => toy.act_t > 0.15 && toy.grounded(),
                    _ => toy.act_t >= toy.act_len,
                };
                if done {
                    self.choose(i, ceiling);
                }
            }
            let toy = &self.toys[i];
            if toy.action == Action::Spin && toy.restyle && toy.act_t >= toy.act_len / 2.0 {
                let outfit = self.new_outfit();
                let toy = &mut self.toys[i];
                toy.outfit = outfit;
                toy.restyle = false;
            }
            let toy = &mut self.toys[i];
            if toy.action == Action::Walk && !toy.asleep() {
                let dir = if toy.target > toy.x { 1.0 } else { -1.0 };
                toy.right = dir > 0.0;
                let pace = scale * if toy.visiting.is_some() { VISIT } else { WALK };
                toy.vx = dir * pace;
                // Never overshoot the target.
                if (toy.target - toy.x).abs() < pace * dt {
                    toy.vx = (toy.target - toy.x) / dt.max(1e-3);
                }
            } else {
                toy.vx = 0.0;
            }
            toy.vy -= GRAVITY * dt;
            toy.y += toy.vy * dt;
            if toy.y <= 0.0 {
                toy.y = 0.0;
                toy.vy = 0.0;
            }
            toy.y = toy.y.min(ceiling.max(0.0));
            toy.x += toy.vx * dt;
            let hi = toy.hi.max(toy.lo);
            if toy.x < toy.lo || toy.x > hi {
                toy.x = toy.x.clamp(toy.lo, hi);
                toy.target = toy.x;
            }
        }
        if self.visits {
            self.visit();
        }
        self.collide();
        self.separate();
        // Visitors back inside their own strip settle down.
        for toy in &mut self.toys {
            if let Some((_, true)) = toy.visiting {
                let home = toy.home;
                if toy.x >= home.0 && toy.x <= home.1 {
                    toy.visiting = None;
                    toy.lo = home.0;
                    toy.hi = home.1;
                }
            }
        }
    }

    /// Now and then a header Clawd walks over to the nearest Clawd on its
    /// row; one on its way keeps heading for its friend.
    fn visit(&mut self) {
        let width = self.width();
        for i in 0..self.toys.len() {
            let (lane, x, id) = (self.toys[i].lane, self.toys[i].x, self.toys[i].id);
            let friend = self
                .toys
                .iter()
                .filter(|other| other.lane == lane && other.id != id)
                .min_by(|a, b| (a.x - x).abs().total_cmp(&(b.x - x).abs()))
                .map(|other| (other.id, other.x, other.home));
            let cooldown = self.range(14.0, 24.0);
            let toy = &mut self.toys[i];
            match (toy.visiting, friend) {
                (Some((target, false)), Some((fid, fx, _))) if target == fid => {
                    // Stop one body short of the friend.
                    toy.target = if fx > toy.x { fx - width - 1.0 } else { fx + width + 1.0 };
                }
                (Some((target, false)), _) => toy.visiting = Some((target, true)),
                (None, Some((fid, fx, fhome)))
                    if toy.visit_cd <= 0.0
                        && !toy.asleep()
                        && !toy.blocked
                        && toy.grounded()
                        && matches!(toy.action, Action::Idle | Action::Walk) =>
                {
                    toy.visit_cd = cooldown;
                    toy.visiting = Some((fid, false));
                    toy.lo = toy.lo.min(fhome.0);
                    toy.hi = toy.hi.max(fhome.1);
                    toy.start(Action::Walk, 0.0);
                    toy.target = if fx > toy.x { fx - width - 1.0 } else { fx + width + 1.0 };
                }
                (None, _) if toy.visit_cd <= 0.0 => toy.visit_cd = cooldown,
                _ => {}
            }
        }
    }

    /// Clawds that bump into each other stop, face each other and wave; a
    /// visitor that reached its friend says hello, then heads home.
    fn collide(&mut self) {
        let (width, height) = (self.width(), self.height());
        let n = self.toys.len();
        for i in 0..n {
            for j in i + 1..n {
                if self.toys[i].lane != self.toys[j].lane {
                    continue;
                }
                let (a, b) = if self.toys[i].x <= self.toys[j].x { (i, j) } else { (j, i) };
                let gap = self.toys[b].x - self.toys[a].x;
                let dy = (self.toys[a].y - self.toys[b].y).abs();
                if gap > width + 1.5 || dy >= height {
                    continue;
                }
                let fresh = self.toys[a].bump_cd <= 0.0 && self.toys[b].bump_cd <= 0.0;
                for (k, faces_right) in [(a, true), (b, false)] {
                    let toy = &mut self.toys[k];
                    if let Some((target, false)) = toy.visiting {
                        toy.visiting = Some((target, true));
                    }
                    if fresh && !toy.asleep() {
                        toy.right = faces_right;
                        toy.start(Action::Wave, 1.2);
                        toy.target = toy.x;
                        toy.bump_cd = 3.0;
                    }
                }
            }
        }
    }

    /// Pack bodies that share a height band so none overlap: sweep right
    /// pushing each away from its left neighbors, then sweep left from the
    /// far wall, then right again from the near wall.
    fn separate(&mut self) {
        let span = self.width() + 1.0;
        let bh = self.height();
        let mut order: Vec<usize> = (0..self.toys.len()).collect();
        order.sort_by(|&i, &j| {
            (self.toys[i].lane, self.toys[i].x)
                .partial_cmp(&(self.toys[j].lane, self.toys[j].x))
                .unwrap()
        });
        let touching = |a: &Body, b: &Body| a.lane == b.lane && (a.y - b.y).abs() < bh;
        for pass in 0..3 {
            let forward = pass != 1;
            for k in 0..order.len() {
                let k = if forward { k } else { order.len() - 1 - k };
                let i = order[k];
                let mut x = self.toys[i].x;
                for &j in &order {
                    if j == i || !touching(&self.toys[i], &self.toys[j]) {
                        continue;
                    }
                    let before = order.iter().position(|&o| o == j).unwrap() < k;
                    if forward && before {
                        x = x.max(self.toys[j].x + span);
                    } else if !forward && !before {
                        x = x.min(self.toys[j].x - span);
                    }
                }
                let toy = &mut self.toys[i];
                toy.x = if forward { x.min(toy.hi.max(toy.lo)) } else { x.max(toy.lo) };
            }
        }
    }
}

/// A pixel canvas. Pixels are square half cells: `x` is the cell column,
/// `y` runs two per row.
struct Canvas {
    x0: i32,
    w: i32,
    h: i32,
    px: Vec<Option<Color>>,
    /// Text: (cell x, pixel y, text, color).
    text: Vec<(i32, i32, &'static str, Color)>,
}

impl Canvas {
    fn new(x0: i32, w: i32, h: i32) -> Self {
        Self { x0, w, h, px: vec![None; (w * h).max(0) as usize], text: Vec::new() }
    }

    fn get(&self, x: i32, y: i32) -> Option<Color> {
        let x = x - self.x0;
        if (0..self.w).contains(&x) && (0..self.h).contains(&y) {
            self.px[(y * self.w + x) as usize]
        } else {
            None
        }
    }

    fn set(&mut self, x: i32, y: i32, color: Color) {
        let x = x - self.x0;
        if (0..self.w).contains(&x) && (0..self.h).contains(&y) {
            self.px[(y * self.w + x) as usize] = Some(color);
        }
    }
}

/// Draws in sprite units relative to a sprite's origin (body row 0, column
/// 0), `s` pixels per unit, mirrored when `flip` is set.
struct Pen<'a> {
    canvas: &'a mut Canvas,
    ox: i32,
    oy: i32,
    flip: bool,
    s: i32,
}

impl Pen<'_> {
    fn ux(&self, dx: i32) -> i32 {
        if self.flip { W - 1 - dx } else { dx }
    }

    fn p(&mut self, dx: i32, dy: i32, color: Color) {
        let x = self.ox + self.ux(dx) * self.s;
        let y = self.oy + dy * self.s;
        for j in 0..self.s {
            for i in 0..self.s {
                self.canvas.set(x + i, y + j, color);
            }
        }
    }

    /// Pixel art from rows of palette letters, top left at unit (x, y).
    fn art(&mut self, x: i32, y: i32, rows: &[&str]) {
        for (j, row) in rows.iter().enumerate() {
            for (i, ch) in row.chars().enumerate() {
                if let Some(color) = ink(ch) {
                    self.p(x + i as i32, y + j as i32, color);
                }
            }
        }
    }

    /// Text anchored at a unit; text never mirrors.
    fn say(&mut self, dx: i32, dy: i32, text: &'static str, color: Color) {
        let len = text.chars().count() as i32;
        let mut x = self.ox + self.ux(dx) * self.s;
        if self.flip {
            x -= len - 1;
        }
        self.canvas.text.push((x, self.oy + dy * self.s, text, color));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Eyes {
    Open,
    /// Toward the way it faces.
    Look,
    /// Mid-blink: no eyes for a frame.
    Blink,
    /// Closed: dark lids instead of black eyes.
    Shut,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Legs {
    Stand,
    /// Mid-stride: the inner pair down.
    StepA,
    /// Full stride: the outer pair down.
    StepB,
    /// Folded under a sitting, squashed or sleeping body: no legs.
    Folded,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Arms {
    Out,
    /// Swinging with a step: the front arm forward and low, the back one up.
    Swing,
    Up,
    /// The front arm raised high (a wave), or halfway.
    WaveHigh,
    WaveMid,
}

/// Which way the block shows: 3/4 toward where it faces, the front, or its
/// back (no face, all in shade).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum View {
    Side,
    Front,
    Back,
}

/// A whole-body pose: the top face at row `top - tall`, the underside at
/// `top + 3`, the legs at `top + 4`.
#[derive(Clone, Copy, Debug)]
struct Pose {
    top: i32,
    /// Extra rows of body (a stretch).
    tall: i32,
    view: View,
    eyes: Eyes,
    legs: Legs,
    arms: Arms,
}

impl Pose {
    fn stand(view: View, eyes: Eyes) -> Self {
        Self { top: 1, tall: 0, view, eyes, legs: Legs::Stand, arms: Arms::Out }
    }

    /// The row of the top face.
    fn head(&self) -> i32 {
        self.top - self.tall
    }
}

/// Draw the Clawd itself, facing right before mirroring.
fn clawd(pen: &mut Pen, pose: Pose) {
    let t = pose.head();
    let bottom = pose.top + 3;
    // The top face catches the light; its back corner is rounded off (both
    // corners when seen from the front or back).
    let top_cols = if pose.view == View::Side { 2..=6 } else { 2..=5 };
    for x in top_cols {
        pen.p(x, t, LIT);
    }
    for y in t + 1..bottom {
        for x in 1..=6 {
            let color = match pose.view {
                View::Back => SIDE,
                View::Side if x == 1 => SIDE,
                View::Front if x == 1 || x == 6 => SIDE,
                _ => ORANGE,
            };
            pen.p(x, y, color);
        }
    }
    for x in 1..=6 {
        pen.p(x, bottom, UNDER);
    }
    let eyes: &[i32] = match (pose.view, pose.eyes) {
        (View::Back, _) | (_, Eyes::Blink) => &[],
        (View::Front, _) => &[2, 5],
        (View::Side, Eyes::Look) => &[4, 6],
        (View::Side, _) => &[3, 5],
    };
    let eye = if pose.eyes == Eyes::Shut { UNDER } else { INK };
    for &x in eyes {
        pen.p(x, t + 1, eye);
    }
    // Arms: the far one in shade when seen from the side.
    let a = pose.top + 2;
    let (back, front) = match pose.view {
        View::Side => (UNDER, ORANGE),
        View::Front => (ORANGE, ORANGE),
        View::Back => (SIDE, SIDE),
    };
    match pose.arms {
        Arms::Out => {
            pen.p(0, a, back);
            pen.p(7, a, front);
        }
        Arms::Swing => {
            pen.p(0, a - 1, back);
            pen.p(7, a + 1, front);
        }
        Arms::Up => {
            for y in [t - 1, t] {
                pen.p(0, y, back);
                pen.p(7, y, front);
            }
        }
        Arms::WaveHigh => {
            pen.p(0, a, back);
            pen.p(7, t, front);
            pen.p(7, t - 1, front);
        }
        Arms::WaveMid => {
            pen.p(0, a, back);
            pen.p(7, t + 1, front);
        }
    }
    // Legs: the far pair darker than the near pair.
    let leg = |x: i32| match pose.view {
        View::Side if x == 1 || x == 5 => UNDER,
        _ => SIDE,
    };
    let cols: &[i32] = match pose.legs {
        Legs::Stand => &[1, 2, 5, 6],
        Legs::StepA => &[2, 5],
        Legs::StepB => &[1, 6],
        Legs::Folded => &[],
    };
    for &x in cols {
        pen.p(x, pose.top + 4, leg(x));
    }
}

/// The outfit over the body. `head`/`neck` are false where a tool's own
/// hat or suit takes their place; `flap` moves a scarf's or band's tail.
fn dress(pen: &mut Pen, outfit: Outfit, pose: Pose, head: bool, neck: bool, flap: bool) {
    let t = pose.head();
    let n = pose.top + 2;
    let side = pose.view == View::Side;
    let back = pose.view == View::Back;
    let (m, d, a) = (outfit.main, darker(outfit.main), outfit.accent);
    let row = |pen: &mut Pen, y: i32, xs: std::ops::RangeInclusive<i32>, c: Color| {
        for x in xs {
            pen.p(x, y, c);
        }
    };
    if head {
        match outfit.head {
            Head::Bare => {}
            Head::Fedora => {
                // Crown, then a band between the brim's ends.
                row(pen, t - 1, 2..=5, m);
                row(pen, t, 1..=6, d);
                row(pen, t, 2..=5, a);
                if side {
                    pen.p(7, t, d);
                }
            }
            Head::Cap => {
                row(pen, t - 1, 2..=5, m);
                match pose.view {
                    View::Side => {
                        row(pen, t, 1..=5, m);
                        row(pen, t, 6..=7, d);
                    }
                    View::Front => row(pen, t, 1..=6, d),
                    View::Back => row(pen, t, 1..=6, m),
                }
            }
            Head::Beanie => {
                row(pen, t - 1, 2..=5, m);
                row(pen, t, 1..=6, lighter(m));
            }
            Head::Headphones => {
                row(pen, t - 1, 2..=5, CHARCOAL);
                pen.p(1, t, CHARCOAL);
                pen.p(6, t, CHARCOAL);
                pen.p(1, t + 1, m);
                if !side {
                    pen.p(6, t + 1, m);
                }
            }
            Head::Crown => {
                let spikes: &[i32] = if side { &[2, 4, 6] } else { &[2, 5] };
                for &x in spikes {
                    pen.p(x, t - 1, GOLD);
                }
                row(pen, t, 2..=if side { 6 } else { 5 }, GOLD);
                if !back {
                    pen.p(if side { 5 } else { 3 }, t, JEWEL);
                }
            }
            Head::Headband => {
                row(pen, t, 1..=6, WHITE);
                if !back {
                    pen.p(if side { 5 } else { 3 }, t, a);
                }
                if side {
                    pen.p(0, if flap { t } else { t + 1 }, WHITE);
                }
            }
        }
    }
    if outfit.shades && !back && pose.eyes != Eyes::Shut {
        let (x0, glint) = if side { (3, 5) } else { (2, 2) };
        row(pen, t + 1, x0..=x0 + 3, CHARCOAL);
        pen.p(glint, t + 1, WHITE);
    }
    if neck {
        match outfit.neck {
            Neck::Bare => {}
            Neck::Scarf => {
                row(pen, n, 1..=6, m);
                pen.p(if side { 5 } else { 3 }, n, a);
                if side {
                    // The tail trails behind and flaps on the move.
                    if flap {
                        pen.p(-1, n, m);
                    } else {
                        pen.p(0, n + 1, m);
                    }
                } else if !back {
                    pen.p(2, n + 1, m);
                }
            }
            Neck::Bowtie if !back => {
                let x0 = if side { 4 } else { 3 };
                pen.p(x0, n, a);
                pen.p(x0 + 1, n, darker(a));
                if side {
                    pen.p(x0 + 2, n, a);
                }
            }
            Neck::Bowtie => {}
            Neck::Chain if !back => {
                let xs = if side { 3..=6 } else { 2..=5 };
                for x in xs.step_by(2) {
                    pen.p(x, n + 1, GOLD);
                }
            }
            Neck::Chain => {}
        }
    }
}

/// Whether a tool's hat or suit covers the outfit's head and neck.
fn hat_covers(tool: Tool) -> (bool, bool) {
    match tool {
        Tool::Build | Tool::Skill => (true, false),
        Tool::Delegate => (false, true),
        _ => (false, false),
    }
}

/// A hat or suit for the tools that come with one.
fn hat(pen: &mut Pen, tool: Tool, pose: Pose) {
    let t = pose.head();
    let n = pose.top + 2;
    match tool {
        Tool::Build => {
            for x in 2..=5 {
                pen.p(x, t - 1, Color::Rgb(0xF2, 0xC2, 0x30));
            }
            for x in 1..=7 {
                pen.p(x, t, Color::Rgb(0xC9, 0x96, 0x1A));
            }
        }
        Tool::Skill => {
            for x in 1..=5 {
                pen.p(x, t - 1, Color::Rgb(0x2F, 0x2A, 0x3A));
            }
        }
        Tool::Delegate => {
            let tie = if pose.view == View::Side { 5 } else { 3 };
            for y in [n, n + 1] {
                for x in 1..=6 {
                    pen.p(x, y, Color::Rgb(0x2A, 0x2A, 0x33));
                }
                if pose.view != View::Back {
                    pen.p(tie, y, Color::Rgb(0xC0, 0x39, 0x2B));
                }
            }
        }
        _ => {}
    }
}

/// The tool in the front hand. `alt` is the tool's second frame while it is
/// used; `t` the time into the action (a rocket launches on a cycle).
fn prop(pen: &mut Pen, tool: Tool, top: i32, alt: bool, working: bool, t: f32) {
    let y = top - 1;
    let (x, x1) = (W, W + 1);
    match tool {
        Tool::Read => {
            if alt {
                pen.art(x, y, &[".GG", "GwLG", "GLLG", "BGG"]);
            } else {
                pen.art(x, y, &[".GG", "GLwG", "GLLG", "BGG"]);
            }
        }
        Tool::Edit => {
            let y = if alt { y + 1 } else { y };
            pen.art(x, y, &["p", "P", "P", "K"]);
        }
        Tool::Build => {
            if alt {
                pen.art(x, y + 2, &["..S", "BBS", "..Sf"]);
            } else {
                pen.art(x, y, &["SSS", ".B", ".B", "B"]);
            }
        }
        Tool::Test => {
            if alt {
                pen.art(x, y, &[".WWW", ".WgW", ".B", "B"]);
            } else {
                pen.art(x, y, &[".WWW", ".WMW", ".B.g", "B"]);
            }
        }
        Tool::Ship => {
            // Held with a flickering flame, then off it goes and comes back.
            let cycle = t % 3.0;
            let rise = if working && cycle > 2.2 { ((cycle - 2.2) * 14.0) as i32 } else { 0 };
            let flame = if alt { "f" } else { "F" };
            pen.art(x, y - rise, &[".R", ".W", ".A", "RWR"]);
            pen.art(x1, y + 4 - rise, &[flame]);
        }
        Tool::Web => {
            if alt {
                pen.art(x, y, &[".CC", "ACCC", "A"]);
            } else {
                pen.art(x, y, &[".CC", "CCCC", "A", "A"]);
            }
        }
        Tool::Delegate => {
            let bx = if alt { x1 } else { x };
            pen.art(x, y + 3, &["B"]);
            pen.art(bx, y + 4, &["bbb", "bbb"]);
        }
        Tool::Skill => {
            if alt {
                pen.art(x, y, &[".u", "B", "B", "B"]);
            } else {
                pen.art(x, y, &["u", "B", "B", "B"]);
            }
        }
        Tool::Shell => {
            if alt {
                pen.art(x, y, &["SS", ".SS", ".s", "s"]);
            } else {
                pen.art(x, y, &["S.S", "SSS", ".s", "s"]);
            }
        }
    }
}

/// Draw one Clawd with its feet on pixel row `ground`, `s` pixels per unit.
fn draw_toy(canvas: &mut Canvas, toy: &Body, s: i32, x_off: i32, ground: i32, clock: f32) {
    let tick = ((clock + toy.phase) * 10.0) as u32;
    let blink = tick % 38 == 0;
    let open = if blink { Eyes::Blink } else { Eyes::Open };
    let look = if blink { Eyes::Blink } else { Eyes::Look };
    let oy = ground - H * s + 1 - toy.y.round() as i32;
    let ox = x_off + toy.x.round() as i32;
    let t = toy.act_t;
    // Every other `len` seconds into the action.
    let beat = |len: f32| ((t / len) as i32).rem_euclid(2) == 1;
    let flap = ((clock + toy.phase) / 0.25) as i32 % 2 == 1;
    if toy.asleep() {
        // Down on its belly, eyes shut, still dressed.
        let pose = Pose {
            top: 2,
            tall: 0,
            view: View::Side,
            eyes: Eyes::Shut,
            legs: Legs::Folded,
            arms: Arms::Out,
        };
        let mut pen = Pen { canvas, ox, oy, flip: !toy.right, s };
        clawd(&mut pen, pose);
        dress(&mut pen, toy.outfit, pose, true, true, false);
        let z = ((clock * 1.2) as i32) % 3;
        pen.say(W + z, 1 - z, if z == 2 { "Z" } else { "z" }, SKY);
        return;
    }
    let mut right = toy.right;
    let mut pose = Pose::stand(View::Side, open);
    if !toy.grounded() {
        pose.arms = Arms::Up;
    } else {
        match toy.action {
            Action::Hop => {}
            Action::Idle => pose.view = View::Front,
            Action::Land => {
                pose.top = 2;
                pose.legs = Legs::Folded;
            }
            Action::Walk => {
                if toy.vx.abs() > 0.5 {
                    let step = (toy.x / s as f32).round() as i32;
                    if step.rem_euclid(2) == 0 {
                        pose.legs = Legs::StepA;
                        pose.arms = Arms::Swing;
                    } else {
                        pose.legs = Legs::StepB;
                    }
                }
                pose.eyes = look;
            }
            Action::Look | Action::Work => pose.eyes = look,
            Action::Sit => {
                pose.top = 2;
                pose.legs = Legs::Folded;
            }
            Action::Stretch => {
                if t > 0.15 && t < toy.act_len - 0.15 {
                    pose.tall = 1;
                    pose.arms = Arms::Up;
                    pose.eyes = Eyes::Shut;
                }
            }
            Action::Wave => {
                pose.arms = if beat(0.2) { Arms::WaveMid } else { Arms::WaveHigh };
            }
            Action::Dance => {
                // Up with arms high, then down low; turn every other beat.
                let n = (t / 0.3) as i32;
                if (n / 2) % 2 == 1 {
                    right = !right;
                }
                if n % 2 == 1 {
                    pose.top = 2;
                    pose.legs = Legs::Folded;
                } else {
                    pose.arms = Arms::Up;
                }
            }
            Action::Spin => match ((t / 0.1) as i32).rem_euclid(4) {
                0 => {}
                1 => pose.view = View::Front,
                2 => right = !right,
                _ => pose.view = View::Back,
            },
            Action::Cheer => {
                pose.view = View::Front;
                pose.arms = Arms::Up;
                if beat(0.2) {
                    pose.top = 2;
                    pose.legs = Legs::Folded;
                }
            }
        }
    }
    let mut pen = Pen { canvas, ox, oy, flip: !right, s };
    let working = toy.action == Action::Work && toy.grounded();
    let alt = working && ((clock + toy.phase) / 0.3) as i32 % 2 == 1;
    let flap = flap && (toy.action == Action::Walk || toy.action == Action::Dance);
    match (toy.tool, toy.blocked) {
        (Some(tool), false) => {
            if pose.arms == Arms::Up {
                pose.arms = Arms::Out;
            }
            clawd(&mut pen, pose);
            let (hat_on, suit_on) = hat_covers(tool);
            dress(&mut pen, toy.outfit, pose, !hat_on, !suit_on, flap);
            hat(&mut pen, tool, pose);
            if pose.view != View::Back {
                prop(&mut pen, tool, pose.top, alt, working, t);
            }
        }
        _ => {
            clawd(&mut pen, pose);
            dress(&mut pen, toy.outfit, pose, true, true, flap);
        }
    }
}

/// The 3D pose of `toy` this frame and whether its turn is driven directly
/// (a spin) rather than eased.
fn pose3(toy: &Body, clock: f32) -> (super::toy3d::Pose3, bool) {
    use super::toy3d::{Arm, Eyes as Eyes3, Pose3};
    let tick = ((clock + toy.phase) * 10.0) as u32;
    let blink = tick % 38 == 0;
    let t = toy.act_t;
    let beat = |len: f32| ((t / len) as i32).rem_euclid(2) == 1;
    let side = |right: bool| if right { 50.0 } else { -50.0 };
    let mut pose = Pose3::stand(side(toy.right));
    if blink {
        pose.eyes = Eyes3::Shut;
    }
    if toy.asleep() {
        pose.flat = true;
        pose.eyes = Eyes3::Shut;
        pose.arms = (Arm::Tucked, Arm::Tucked);
        pose.yaw = side(toy.right) * 0.6;
        return (pose, false);
    }
    if !toy.grounded() {
        pose.arms = (Arm::Up, Arm::Up);
        return (pose, false);
    }
    match toy.action {
        Action::Hop | Action::Look => {}
        Action::Idle => pose.yaw = side(toy.right) * 0.35,
        Action::Land | Action::Sit => pose.low = true,
        Action::Walk => {
            if toy.vx.abs() > 0.5 {
                let step = (toy.x * 2.0).round() as i32;
                let a = step.rem_euclid(2) == 0;
                pose.lift = if a { [1, 0, 1, 0] } else { [0, 1, 0, 1] };
                pose.arms = if a { (Arm::Mid, Arm::Out) } else { (Arm::Out, Arm::Mid) };
            }
        }
        Action::Work => {
            let alt = ((clock + toy.phase) / 0.3) as i32 % 2 == 1;
            pose.arms = if alt { (Arm::Out, Arm::Mid) } else { (Arm::Mid, Arm::Out) };
        }
        Action::Stretch => {
            if t > 0.15 && t < toy.act_len - 0.15 {
                pose.tall = true;
                pose.arms = (Arm::Up, Arm::Up);
                pose.eyes = Eyes3::Shut;
            }
        }
        Action::Wave => {
            pose.yaw = side(toy.right) * 0.4;
            let hand = if beat(0.2) { Arm::Mid } else { Arm::Up };
            pose.arms = if toy.right { (Arm::Out, hand) } else { (hand, Arm::Out) };
        }
        Action::Dance => {
            let n = (t / 0.3) as i32;
            let right = if (n / 2) % 2 == 1 { !toy.right } else { toy.right };
            pose.yaw = side(right) * 0.7;
            if n % 2 == 1 {
                pose.low = true;
            } else {
                pose.arms = (Arm::Up, Arm::Up);
            }
        }
        Action::Spin => {
            let k = (t / toy.act_len.max(0.1)).clamp(0.0, 1.0);
            pose.yaw = side(toy.right) + 360.0 * k;
            return (pose, true);
        }
        Action::Cheer => {
            pose.yaw = side(toy.right) * 0.2;
            pose.arms = (Arm::Up, Arm::Up);
            pose.low = beat(0.2);
        }
    }
    (pose, false)
}

impl ToyWorld {
    /// Ease each toy's turn toward its pose and return the poses to draw.
    fn poses3(&mut self, dt: f32) -> Vec<super::toy3d::Pose3> {
        let clock = self.clock;
        let mut out = Vec::with_capacity(self.toys.len());
        let mut yaws = Vec::with_capacity(self.toys.len());
        for toy in &self.toys {
            let (mut pose, direct) = pose3(toy, clock);
            let prev = self.yaws.iter().find(|(id, _)| *id == toy.id).map(|(_, y)| *y);
            if let (Some(prev), false) = (prev, direct) {
                let mut diff = (pose.yaw - prev).rem_euclid(360.0);
                if diff > 180.0 {
                    diff -= 360.0;
                }
                pose.yaw = prev + diff * (dt * 12.0).min(1.0);
            }
            yaws.push((toy.id, pose.yaw.rem_euclid(360.0)));
            out.push(pose);
        }
        self.yaws = yaws;
        out
    }
}

/// Pixels of one sprite unit (a cell wide, half a cell tall) in a Sixel image.
const UNIT: f32 = 10.0;

/// Sixel rail: one image over the floor area, Clawds standing on its bottom.
fn draw_rail_sixel(world: &mut ToyWorld, buf: &mut Buffer, area: Rect, bg: Color, dt: f32) {
    use super::toy3d::{self, Image, Job};
    // Never touch the screen's last row: an image there scrolls the host.
    let screen = *buf.area();
    let mut rect = area;
    if rect.y + rect.height >= screen.y + screen.height {
        rect.height = (screen.y + screen.height).saturating_sub(rect.y + 1);
    }
    if rect.height < 2 || rect.width == 0 {
        return;
    }
    clear_cells(buf, rect, bg);
    let poses = world.poses3(dt);
    let (w, h) = (u32::from(rect.width) * toy3d::CELL_W, u32::from(rect.height) * toy3d::CELL_H);
    let mut image = Image::new(w, h, toy3d::rgb(bg));
    let ground = h as f32 - 8.0;
    let scale = if h >= 70 { 3.0 } else { 2.2 };
    for toy in &world.toys {
        let cx = (toy.x + W as f32 / 2.0) * UNIT;
        image.shadow(cx, ground, 8.0 * scale);
    }
    for (toy, pose) in world.toys.iter().zip(&poses) {
        let cx = (toy.x + W as f32 / 2.0) * UNIT;
        toy3d::draw_clawd(&mut image, pose, cx, ground, toy.y * UNIT, scale);
    }
    image.outline();
    for toy in world.toys.iter().filter(|toy| toy.asleep()) {
        let x = ((toy.x + W as f32) * UNIT) as i32;
        let z = ((world.clock * 1.2) as i32) % 3;
        image.zed(x + 4 * z, (ground - 14.0 * scale) as i32 - 6 * z, 5 + 2 * (z / 2));
    }
    world.jobs.push(Job { rect, image });
}

/// Sixel header: one image per agent pane's deck, Clawds standing on the
/// deck's bottom edge, visitors drawn wherever they are.
fn draw_header_sixel(world: &mut ToyWorld, buf: &mut Buffer, strips: &[HeaderStrip], dt: f32) {
    use super::toy3d::{self, Image, Job};
    let poses = world.poses3(dt);
    for strip in strips {
        let rect = strip.deck;
        if rect.height == 0 || rect.width == 0 {
            continue;
        }
        let bg = buf.cell((rect.x, rect.y)).map_or(Color::Reset, |cell| cell.bg);
        clear_cells(buf, rect, bg);
        let (w, h) = (u32::from(rect.width) * toy3d::CELL_W, u32::from(rect.height) * toy3d::CELL_H);
        let mut image = Image::new(w, h, toy3d::rgb(bg));
        let ground = h as f32 - 5.0;
        let scale = 2.0;
        let left = f32::from(rect.x);
        let right = f32::from(rect.x + rect.width);
        let mine: Vec<usize> = (0..world.toys.len())
            .filter(|&i| {
                let toy = &world.toys[i];
                toy.lane == strip.rect.y && toy.x + W as f32 > left && toy.x < right
            })
            .collect();
        for &i in &mine {
            let cx = (world.toys[i].x - left + W as f32 / 2.0) * UNIT;
            image.shadow(cx, ground, 8.0 * scale);
        }
        for &i in &mine {
            let toy = &world.toys[i];
            let cx = (toy.x - left + W as f32 / 2.0) * UNIT;
            toy3d::draw_clawd(&mut image, &poses[i], cx, ground, toy.y * UNIT, scale);
        }
        image.outline();
        for &i in &mine {
            let toy = &world.toys[i];
            if toy.asleep() {
                let x = ((toy.x - left + W as f32) * UNIT) as i32 - 6;
                let z = ((world.clock * 1.2) as i32) % 3;
                image.zed(x + 3 * z, 12 - 4 * z, 4 + z / 2);
            }
        }
        world.jobs.push(Job { rect, image });
    }
}

/// Blank cells under a Sixel image, in the image's background color.
fn clear_cells(buf: &mut Buffer, rect: Rect, bg: Color) {
    for y in rect.y..rect.y + rect.height {
        for x in rect.x..rect.x + rect.width {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.reset();
                cell.set_bg(bg);
            }
        }
    }
}

/// Half-block render of `canvas` into `area`: each cell shows its top pixel
/// over its bottom one. `allow` limits which cells (absolute x, y) may change.
fn blit(
    buf: &mut Buffer,
    area: Rect,
    canvas: &Canvas,
    bg: Option<Color>,
    dim: Style,
    allow: &dyn Fn(u16, u16) -> bool,
) {
    for cy in 0..area.height {
        for cx in 0..area.width {
            let (ax, ay) = (area.x + cx, area.y + cy);
            if !allow(ax, ay) {
                continue;
            }
            let (x, y) = (i32::from(ax), 2 * i32::from(cy));
            let (top, bottom) = (canvas.get(x, y), canvas.get(x, y + 1));
            let cell = &mut buf[(ax, ay)];
            let under = bg.unwrap_or(cell.bg);
            let (glyph, fg, back) = match (top, bottom) {
                (None, None) => continue,
                (Some(t), None) => ("▀", t, under),
                (None, Some(b)) => ("▄", b, under),
                (Some(t), Some(b)) if t == b => ("█", t, under),
                (Some(t), Some(b)) => ("▀", t, b),
            };
            cell.set_symbol(glyph).set_style(Style::default().fg(fg).bg(back));
        }
    }
    for &(x, y, text, color) in &canvas.text {
        let cy = y.div_euclid(2);
        if !(0..i32::from(area.height)).contains(&cy) {
            continue;
        }
        let lo = i32::from(area.x);
        let hi = lo + i32::from(area.width);
        let len = text.chars().count() as i32;
        let start = x.clamp(lo, (hi - len).max(lo));
        for (k, ch) in text.chars().enumerate() {
            let ax = start + k as i32;
            let ay = area.y + cy as u16;
            if ax >= hi || !allow(ax as u16, ay) {
                continue;
            }
            let cell = &mut buf[(ax as u16, ay)];
            let under = bg.unwrap_or(cell.bg);
            cell.set_symbol(ch.encode_utf8(&mut [0; 4])).set_style(dim.fg(color).bg(under));
        }
    }
}

/// A floor shadow a little lighter (on dark themes) or darker than `bg`.
fn shadow(bg: Color) -> Option<Color> {
    let Color::Rgb(r, g, b) = bg else { return None };
    let light = u32::from(r) + u32::from(g) + u32::from(b) < 384;
    let mix = |c: u8| if light { c + (255 - c) / 9 } else { c - c / 9 };
    Some(Color::Rgb(mix(r), mix(g), mix(b)))
}

/// Whether `area` is big enough to hold a rail toy.
pub(crate) fn fits(area: Rect) -> bool {
    i32::from(area.width) >= W + 2 * PROP && i32::from(area.height) >= 4
}

/// Step the rail world and draw it into `area`. `agents` lists every agent
/// and its status: working and blocked ones get a Clawd (holding the tool
/// from `tools`), else one Clawd naps.
pub(crate) fn draw_rail(
    world: &mut ToyWorld,
    buf: &mut Buffer,
    area: Rect,
    agents: &[(SurfaceId, AgentStatus)],
    tools: &[(SurfaceId, Tool)],
    bg: Color,
    dim: Style,
) {
    if !fits(area) {
        return;
    }
    let w = i32::from(area.width);
    let h = 2 * i32::from(area.height);
    // One size everywhere: small, like the header Clawds.
    let s = 1;
    world.set_scale(s);
    world.visits = false;
    let (lo, hi) = ((PROP * s) as f32, (w - (W + PROP) * s).max(PROP * s) as f32);
    // As many Clawds as the floor holds side by side.
    let room = (w / ((W + PROP) * s + 1)).max(1) as usize;
    let mut ids: Vec<(SurfaceId, AgentStatus)> = agents
        .iter()
        .filter(|agent| matches!(agent.1, AgentStatus::Working | AgentStatus::Blocked))
        .copied()
        .collect();
    ids.sort_unstable_by_key(|agent| agent.0);
    ids.dedup_by_key(|agent| agent.0);
    let mut wanted: Vec<Want> = ids
        .into_iter()
        .take(MAX_TOYS.min(room))
        .map(|(id, status)| {
            let mood = if status == AgentStatus::Blocked {
                Mood::Blocked
            } else {
                Mood::Working(tools.iter().find(|tool| tool.0 == id).map(|tool| tool.1))
            };
            Want { id, lane: 0, lo, hi, mood }
        })
        .collect();
    if wanted.is_empty() {
        // Nobody works: one Clawd naps a few seconds, then wakes up.
        wanted.push(Want { id: NAPPER, lane: 0, lo, hi, mood: Mood::Working(None) });
    }
    // Room overhead for the hat and a raised tool.
    let ceiling = (h - (H + 2) * s).clamp(0, 4 * s) as f32;
    world.sync(&wanted);
    let dt = world.tick(Instant::now());
    world.step(dt, ceiling);
    if super::toy3d::enabled() {
        draw_rail_sixel(world, buf, area, bg, dt);
        return;
    }
    let x0 = i32::from(area.x);
    let mut canvas = Canvas::new(x0, w, h);
    // Feet one pixel above the bottom, a soft shadow on the floor below.
    let ground = h - 2;
    for toy in &world.toys {
        draw_toy(&mut canvas, toy, s, x0, ground, world.clock);
    }
    if let Some(shade) = shadow(bg) {
        for toy in &world.toys {
            let left = x0 + toy.x.round() as i32;
            // Under the body; smaller while it is up in the air.
            let (a, b) = if toy.asleep() {
                (0, W)
            } else if toy.y > 1.0 {
                (2, W - 2)
            } else {
                (1, W - 1)
            };
            for x in left + a..left + b {
                canvas.set(x, h - 1, shade);
            }
        }
    }
    blit(buf, area, &canvas, Some(bg), dim, &|_, _| true);
}

/// Step the header world and draw a Clawd for every agent pane: it stands
/// on the free cells of the tab bar row with its body in the deck rows above,
/// holds the tool its agent uses, and walks over to visit Clawds of neighbor
/// panes on the same row. Returns whether any toy was drawn.
pub(crate) fn draw_header(
    world: &mut ToyWorld,
    buf: &mut Buffer,
    strips: &[HeaderStrip],
    _agents: &[(SurfaceId, AgentStatus)],
    dim: Style,
) -> bool {
    world.set_scale(1);
    world.visits = true;
    let strips: Vec<_> =
        strips.iter().filter(|strip| i32::from(strip.rect.width) >= W + 3).copied().collect();
    let wanted: Vec<Want> = strips
        .iter()
        .map(|strip| {
            let lo = f32::from(strip.rect.x) + 1.0;
            let hi = f32::from(strip.rect.x + strip.rect.width) - W as f32 - 1.0;
            let mood = match strip.status {
                AgentStatus::Idle => Mood::Idle,
                AgentStatus::Blocked => Mood::Blocked,
                AgentStatus::Working => Mood::Working(detect_tool(buf, strip.body)),
                _ => Mood::Working(None),
            };
            Want { id: strip.surface, lane: strip.rect.y, lo, hi: hi.max(lo), mood }
        })
        .collect();
    world.sync(&wanted);
    if world.toys.is_empty() {
        return false;
    }
    let dt = world.tick(Instant::now());
    world.step(dt, 0.0);
    if super::toy3d::enabled() {
        draw_header_sixel(world, buf, &strips, dt);
        return true;
    }
    let mut rows: Vec<u16> = strips.iter().map(|strip| strip.rect.y).collect();
    rows.sort_unstable();
    rows.dedup();
    let screen = *buf.area();
    let deck_rows = crate::app::TOY_DECK_ROWS;
    for row in rows {
        let Some(top) = row.checked_sub(deck_rows) else { continue };
        let mut canvas = Canvas::new(0, i32::from(screen.x + screen.width), HEADER_PX);
        let ground = HEADER_PX - 1;
        let mut visitor_span: Vec<(u16, u16)> = Vec::new();
        for toy in world.toys.iter().filter(|toy| toy.lane == row) {
            draw_toy(&mut canvas, toy, 1, 0, ground, world.clock);
            if toy.visiting.is_some() {
                let a = toy.x.round() as i32 - 1;
                let z = toy.x.round() as i32 + W + 1;
                visitor_span.push((a.max(0) as u16, z.max(0) as u16));
            }
        }
        let row_strips: Vec<&HeaderStrip> =
            strips.iter().filter(|strip| strip.rect.y == row).collect();
        // Deck rows: anywhere above an agent pane. Tab bar row: only the
        // free cells (and wherever a visitor walks between panes).
        let area = Rect { x: screen.x, y: top, width: screen.width, height: deck_rows + 1 };
        let allow_cell = |x: u16, y: u16| {
            if y < row {
                row_strips
                    .iter()
                    .any(|strip| x >= strip.deck.x && x < strip.deck.x + strip.deck.width)
            } else {
                row_strips
                    .iter()
                    .any(|strip| x >= strip.rect.x && x < strip.rect.x + strip.rect.width)
                    || visitor_span.iter().any(|&(a, b)| x >= a && x < b)
            }
        };
        blit(buf, area, &canvas, None, dim, &allow_cell);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn art_of(canvas: &Canvas, w: i32, h: i32) -> String {
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| match canvas.get(x, y) {
                        Some(INK) => 'o',
                        Some(ORANGE) => '#',
                        Some(LIT) => 'h',
                        Some(SIDE) => 's',
                        Some(UNDER) => 'u',
                        Some(_) => '+',
                        None => '.',
                    })
                    .collect::<String>()
                    + "\n"
            })
            .collect()
    }

    /// From the front: a lit, rounded top, shaded edges, a dark underside,
    /// two pairs of legs, and a free row on top for a hat.
    #[test]
    fn toy_front_pose_is_a_shaded_block() {
        let mut canvas = Canvas::new(0, W, H);
        let mut pen = Pen { canvas: &mut canvas, ox: 0, oy: 0, flip: false, s: 1 };
        clawd(&mut pen, Pose::stand(View::Front, Eyes::Open));
        assert_eq!(
            art_of(&canvas, W, H),
            "........\n\
             ..hhhh..\n\
             .so##os.\n\
             #s####s#\n\
             .uuuuuu.\n\
             .ss..ss.\n"
        );
    }

    /// In 3/4 view the back edge and far limbs are in shade and the eyes
    /// sit toward the front; mirrored when it faces left.
    #[test]
    fn toy_side_pose_turns_its_shade_away() {
        let mut canvas = Canvas::new(0, W, H);
        let mut pen = Pen { canvas: &mut canvas, ox: 0, oy: 0, flip: false, s: 1 };
        clawd(&mut pen, Pose::stand(View::Side, Eyes::Look));
        let right = art_of(&canvas, W, H);
        assert_eq!(
            right,
            "........\n\
             ..hhhhh.\n\
             .s##o#o.\n\
             us######\n\
             .uuuuuu.\n\
             .us..us.\n"
        );
        let mut canvas = Canvas::new(0, W, H);
        let mut pen = Pen { canvas: &mut canvas, ox: 0, oy: 0, flip: true, s: 1 };
        clawd(&mut pen, Pose::stand(View::Side, Eyes::Look));
        let left = art_of(&canvas, W, H);
        for (r, l) in right.lines().zip(left.lines()) {
            assert_eq!(r.chars().rev().collect::<String>(), l);
        }
    }

    /// Every outfit stays inside the sprite's own box, in every view.
    #[test]
    fn toy_outfits_stay_on_the_body() {
        let mut world = ToyWorld::default();
        for _ in 0..200 {
            let outfit = world.new_outfit();
            for view in [View::Side, View::Front, View::Back] {
                let mut canvas = Canvas::new(-2, W + 4, H);
                let mut pen = Pen { canvas: &mut canvas, ox: 0, oy: 0, flip: false, s: 1 };
                let pose = Pose::stand(view, Eyes::Open);
                clawd(&mut pen, pose);
                dress(&mut pen, outfit, pose, true, true, true);
                for y in 0..H {
                    assert_eq!(canvas.get(-2, y), None, "{outfit:?} {view:?} spills left");
                    assert_eq!(canvas.get(W + 1, y), None, "{outfit:?} {view:?} spills right");
                }
            }
        }
    }

    /// At scale two every unit is a 2x2 block of square pixels.
    #[test]
    fn toy_scale_two_doubles_every_unit() {
        let mut small = Canvas::new(0, W, H);
        clawd(
            &mut Pen { canvas: &mut small, ox: 0, oy: 0, flip: false, s: 1 },
            Pose::stand(View::Side, Eyes::Open),
        );
        let mut big = Canvas::new(0, 2 * W, 2 * H);
        clawd(
            &mut Pen { canvas: &mut big, ox: 0, oy: 0, flip: false, s: 2 },
            Pose::stand(View::Side, Eyes::Open),
        );
        for y in 0..2 * H {
            for x in 0..2 * W {
                assert_eq!(big.get(x, y), small.get(x / 2, y / 2), "at {x},{y}");
            }
        }
    }

    fn wanted(id: SurfaceId, lo: f32, hi: f32, mood: Mood) -> Want {
        Want { id, lane: 0, lo, hi, mood }
    }

    #[test]
    fn toy_does_every_move_and_stays_on_its_floor() {
        let mut world = ToyWorld::default();
        world.sync(&[wanted(1, 0.0, 60.0, Mood::Working(None))]);
        let mut seen = Vec::new();
        for _ in 0..6000 {
            world.clock += 0.08;
            world.step(0.08, 4.0);
            let toy = &world.toys[0];
            assert!(toy.x >= -0.01 && toy.x <= 60.01, "left the floor: {}", toy.x);
            assert!(toy.y >= 0.0 && toy.y <= 4.0);
            if !seen.contains(&toy.action) {
                seen.push(toy.action);
            }
        }
        for action in [
            Action::Idle,
            Action::Walk,
            Action::Look,
            Action::Sit,
            Action::Stretch,
            Action::Hop,
            Action::Land,
            Action::Dance,
            Action::Spin,
        ] {
            assert!(seen.contains(&action), "never did {action:?}; did {seen:?}");
        }
    }

    #[test]
    fn toy_uses_its_tool_and_waves_when_blocked() {
        let mut world = ToyWorld::default();
        world.sync(&[wanted(1, 0.0, 60.0, Mood::Working(None))]);
        world.sync(&[wanted(1, 0.0, 60.0, Mood::Working(Some(Tool::Read)))]);
        assert_eq!(world.toys[0].action, Action::Work, "a new tool is put to work at once");
        let mut working = 0;
        for _ in 0..1000 {
            world.clock += 0.08;
            world.step(0.08, 4.0);
            working += usize::from(world.toys[0].action == Action::Work);
        }
        assert!(working > 500, "a Clawd with a tool mostly works ({working}/1000)");
        world.sync(&[wanted(1, 0.0, 60.0, Mood::Blocked)]);
        for _ in 0..100 {
            world.clock += 0.08;
            world.step(0.08, 4.0);
            assert_eq!(world.toys[0].action, Action::Wave, "a blocked agent's Clawd waves");
        }
    }

    #[test]
    fn toy_bodies_never_overlap_on_the_rail_floor() {
        let mut world = ToyWorld::default();
        let wanted: Vec<_> =
            (0..4u64).map(|id| wanted(id, 0.0, 90.0, Mood::Working(None))).collect();
        world.sync(&wanted);
        let width = world.width();
        for _ in 0..2000 {
            world.clock += 0.08;
            world.step(0.08, 4.0);
            for (i, a) in world.toys.iter().enumerate() {
                assert!(a.x >= -0.01 && a.x <= 90.01, "left the floor: {}", a.x);
                for b in &world.toys[i + 1..] {
                    let apart = (a.x - b.x).abs() >= width - 0.01 || (a.y - b.y).abs() >= 4.0;
                    assert!(apart, "toys overlap: {} {} / {} {}", a.x, a.y, b.x, b.y);
                }
            }
        }
    }

    #[test]
    fn toy_header_visitor_goes_home() {
        let mut world = ToyWorld { visits: true, ..ToyWorld::default() };
        let wanted =
            [wanted(1, 0.0, 20.0, Mood::Working(None)), wanted(2, 60.0, 80.0, Mood::Working(None))];
        world.sync(&wanted);
        let mut visited = false;
        for _ in 0..3000 {
            world.clock += 0.08;
            world.step(0.08, 0.0);
            visited |= world.toys.iter().any(|toy| toy.visiting.is_some());
            for toy in &world.toys {
                if toy.visiting.is_none() {
                    let (lo, hi) = if toy.id == 1 { (0.0, 20.0) } else { (60.0, 80.0) };
                    assert!(toy.x >= lo - 0.01 && toy.x <= hi + 0.01, "{} at {}", toy.id, toy.x);
                }
            }
        }
        assert!(visited, "a header Clawd visits its neighbor");
    }

    #[test]
    fn toy_napper_wakes_up_after_a_short_nap() {
        let area = Rect { x: 0, y: 0, width: 24, height: 8 };
        let mut buf = Buffer::empty(ratatui::layout::Rect::new(0, 0, 24, 8));
        let mut world = ToyWorld::default();
        let agents = [(1, AgentStatus::Idle)];
        let mut frames = 0;
        let tick = |world: &mut ToyWorld, buf: &mut Buffer| {
            draw_rail(world, buf, area, &agents, &[], Color::Reset, Style::default());
            world.rewind_for_test(Duration::from_millis(80));
        };
        while frames * 80 < 2_900 {
            tick(&mut world, &mut buf);
            frames += 1;
        }
        assert_eq!(world.toys.len(), 1);
        assert!(world.toys[0].asleep(), "woke before {} ms", NAP.0 * 1000.0);
        while frames * 80 < 5_200 {
            tick(&mut world, &mut buf);
            frames += 1;
        }
        assert!(!world.toys[0].asleep(), "still asleep after {} ms", NAP.1 * 1000.0);
    }

    #[test]
    fn toy_rail_draw_stays_inside_its_area() {
        let area = Rect { x: 2, y: 3, width: 64, height: 12 };
        let mut buf = Buffer::empty(ratatui::layout::Rect::new(0, 0, 70, 20));
        let mut world = ToyWorld::default();
        let agents = [(1, AgentStatus::Working), (2, AgentStatus::Working), (3, AgentStatus::Idle)];
        let tools = [(1, Tool::Build), (2, Tool::Ship)];
        for _ in 0..200 {
            draw_rail(&mut world, &mut buf, area, &agents, &tools, Color::Reset, Style::default());
            world.rewind_for_test(Duration::from_millis(80));
        }
        assert_eq!(world.scale, 1, "rail Clawds are as small as header ones");
        assert_eq!(world.toys.len(), 2);
        for y in 0..20 {
            for x in 0..70 {
                let inside = (2..66).contains(&x) && (3..15).contains(&y);
                if !inside {
                    assert_eq!(buf[(x, y)].symbol(), " ", "drew outside at {x},{y}");
                }
            }
        }
    }

    #[test]
    fn toy_reads_the_tool_from_the_agent_screen() {
        let screen = |lines: &[&str]| {
            let mut buf = Buffer::empty(ratatui::layout::Rect::new(0, 0, 60, 8));
            for (y, line) in lines.iter().enumerate() {
                buf.set_string(0, y as u16, line, Style::default());
            }
            buf
        };
        let body = Rect { x: 0, y: 0, width: 60, height: 8 };
        let cases: [(&[&str], Option<Tool>); 9] = [
            (&["⏺ Read(src/main.rs)", "  ⎿  Read 40 lines"], Some(Tool::Read)),
            (&["● Update(src/app.rs)"], Some(Tool::Edit)),
            (&["⏺ Bash(cargo build --release)"], Some(Tool::Build)),
            (&["⏺ Bash(cargo test -p cmux)"], Some(Tool::Test)),
            (&["⏺ Bash(git push fork main)"], Some(Tool::Ship)),
            (&["⏺ Bash(ls -la)"], Some(Tool::Shell)),
            (&["⏺ Web Search(\"ratatui\")"], Some(Tool::Web)),
            (&["⏺ Read(a.rs)", "", "⏺ I'll fix the bug now."], None),
            (&["$ ls", "a b c"], None),
        ];
        for (lines, tool) in cases {
            assert_eq!(detect_tool(&screen(lines), body), tool, "{lines:?}");
        }
    }

    /// A Clawd mid-move for the sprite dump.
    fn posed(action: Action, t: f32, right: bool, tool: Option<Tool>) -> Body {
        Body {
            id: 1,
            lane: 0,
            lo: 0.0,
            hi: 99.0,
            home: (0.0, 99.0),
            x: 5.0,
            y: 0.0,
            vx: if action == Action::Walk { 5.0 } else { 0.0 },
            vy: 0.0,
            right,
            idle: false,
            blocked: action == Action::Wave,
            tool,
            doze: 0.0,
            nap_after: 0.0,
            outfit: Outfit::PLAIN,
            restyle: false,
            action,
            act_t: t,
            act_len: 1.0,
            target: 5.0,
            bump_cd: 0.0,
            visit_cd: 0.0,
            visiting: None,
            phase: 0.0,
        }
    }

    /// `CMUX_TOY_DUMP=<file.ppm> cargo test toy_dump -- --ignored` renders
    /// every pose and tool, both frames, at both sizes, through the real
    /// half-block renderer, as a PPM picture (8 screen pixels per pixel).
    #[test]
    #[ignore]
    fn toy_dump_frames() {
        let Ok(path) = std::env::var("CMUX_TOY_DUMP") else { return };
        let bg = Color::Rgb(0x0F, 0x14, 0x19);
        let tools = [
            None,
            Some(Tool::Read),
            Some(Tool::Edit),
            Some(Tool::Build),
            Some(Tool::Test),
            Some(Tool::Ship),
            Some(Tool::Web),
            Some(Tool::Delegate),
            Some(Tool::Skill),
            Some(Tool::Shell),
        ];
        let mut tiles: Vec<(Buffer, u16, u16)> = Vec::new();
        let mut world = ToyWorld::default();
        for s in [1] {
            for tool in tools.into_iter().chain([None; 4]) {
                let outfit = world.new_outfit();
                for (action, t, clock, asleep) in [
                    (Action::Work, 0.5, 0.5, false),
                    (Action::Work, 0.5, 0.8, false),
                    (Action::Walk, 0.5, 0.5, false),
                    (Action::Sit, 0.5, 0.5, false),
                    (Action::Stretch, 0.5, 0.5, false),
                    (Action::Wave, 0.1, 0.5, false),
                    (Action::Idle, 0.5, 0.5, false),
                    (Action::Spin, 0.15, 0.5, false),
                    (Action::Spin, 0.35, 0.5, false),
                    (Action::Dance, 0.1, 0.5, false),
                    (Action::Cheer, 0.1, 0.5, false),
                    (Action::Idle, 0.5, 0.5, true),
                ] {
                    let (cols, rows) =
                        (((W + 2 * PROP + 2) * s) as u16, ((H + 3) * s / 2 + 1) as u16);
                    let mut canvas = Canvas::new(0, i32::from(cols), i32::from(rows) * 2);
                    let mut toy = posed(action, t, tool.is_none() || action != Action::Walk, tool);
                    toy.x = (PROP * s + s) as f32;
                    toy.outfit = outfit;
                    if asleep {
                        toy.doze = 1.0;
                    }
                    draw_toy(&mut canvas, &toy, s, 0, i32::from(rows) * 2 - 1, clock);
                    let mut buf = Buffer::empty(ratatui::layout::Rect::new(0, 0, cols, rows));
                    let area = Rect { x: 0, y: 0, width: cols, height: rows };
                    blit(&mut buf, area, &canvas, Some(bg), Style::default(), &|_, _| true);
                    tiles.push((buf, cols, rows));
                }
            }
        }
        // Lay the tiles out 12 to a row; one cell is 8x16 screen pixels.
        let per_row = 12;
        let tile_w = tiles.iter().map(|t| t.1).max().unwrap() as usize * 8 + 8;
        let tile_h = tiles.iter().map(|t| t.2).max().unwrap() as usize * 16 + 8;
        let (img_w, img_h) = (tile_w * per_row, tile_h * tiles.len().div_ceil(per_row));
        let rgb = |c: Color| match c {
            Color::Rgb(r, g, b) => [r, g, b],
            _ => [0x0F, 0x14, 0x19],
        };
        let mut img = vec![0x0Fu8; img_w * img_h * 3];
        for (n, (buf, cols, rows)) in tiles.iter().enumerate() {
            let (tx, ty) = ((n % per_row) * tile_w, (n / per_row) * tile_h);
            for cy in 0..*rows {
                for cx in 0..*cols {
                    let cell = &buf[(cx, cy)];
                    let (top, bottom) = match cell.symbol() {
                        "▀" => (cell.fg, cell.bg),
                        "▄" => (cell.bg, cell.fg),
                        "█" => (cell.fg, cell.fg),
                        _ => (bg, bg),
                    };
                    for py in 0..16 {
                        for px in 0..8 {
                            let c = rgb(if py < 8 { top } else { bottom });
                            let (x, y) =
                                (tx + usize::from(cx) * 8 + px, ty + usize::from(cy) * 16 + py);
                            let at = (y * img_w + x) * 3;
                            img[at..at + 3].copy_from_slice(&c);
                        }
                    }
                }
            }
        }
        let mut out = format!("P6\n{img_w} {img_h}\n255\n").into_bytes();
        out.extend_from_slice(&img);
        std::fs::write(path, out).unwrap();
    }
}
