//! Toys: Clawd, the Claude mascot, living in the chrome.
//!
//! The sprite and its moves follow the Claude desktop app's mascot (and the
//! Claude Code banner, `▐▛███▜▌ / ▝▜█████▛▘ / ▘▘ ▝▝`): a flat Claude-orange
//! block with two eye notches, side arms and four thin legs. It idles and
//! blinks, glances aside, stretches up on its toes with its ears out, turns
//! three quarters (its back side in shade) to walk or sit, hops, and plays
//! with a computer mouse on its cable: swings it over its head, drops it
//! behind, then turns and sits by it.
//!
//! With the sidebar open, Clawds live on the floor of the workspace rail's
//! empty space, one per working agent (a single Clawd naps when no agent
//! works). Every pane that hosts an agent also gets a smaller Clawd in the
//! deck above its tab bar. Toys share a floor per row: they never overlap,
//! turn to look at whoever bumps into them, and header Clawds walk over to
//! visit a neighbor pane's Clawd and come home. A Clawd naps a few seconds
//! when it appears idle or its agent turns idle, then wakes with a stretch.
//!
//! Pixels are quarter cells (`▘▝▀▖▌▞▛▗▚▐▜▄▙▟█`): two across and two down per
//! terminal cell, as in the Claude Code banner.

use cmux_tui_core::{AgentStatus, Rect, SurfaceId};
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Style};
use std::time::{Duration, Instant};

/// Repaint cadence while a toy is on screen.
pub(crate) const FRAME: Duration = Duration::from_millis(80);

/// The smallest rail area worth drawing into.
const MIN_WIDTH: u16 = 10;
const MIN_HEIGHT: u16 = 3;
/// Never crowd the rail with more than this many toys.
const MAX_TOYS: usize = 7;
/// A header toy's world is the pane's deck rows plus its tab bar row.
const DECK_PX: i32 = 2 * (crate::app::TOY_DECK_ROWS as i32 + 1);
/// Gravity in pixels per second squared.
const GRAVITY: f32 = 60.0;
/// Walking speed and a visitor's brisker pace, in pixels per second.
const WALK: f32 = 7.0;
const VISIT: f32 = 22.0;
/// The mouse-and-cable game lasts this long.
const PLAY: f32 = 3.4;
/// A Clawd naps this long (seconds) when it appears idle or turns idle.
const NAP: (f32, f32) = (3.0, 5.0);

/// Claude orange, its shaded side, the eyes, and the mouse's cable.
const ORANGE: Color = Color::Rgb(0xD9, 0x77, 0x57);
const SIDE: Color = Color::Rgb(0xB5, 0x60, 0x47);
const INK: Color = Color::Rgb(0x16, 0x12, 0x10);
const CORD: Color = Color::Rgb(0x8E, 0x94, 0x9E);
const MOUSE: Color = Color::Rgb(0xB4, 0xBA, 0xC2);
const SKY: Color = Color::Rgb(0x81, 0xD4, 0xFA);

/// A Clawd's proportions, facing right (a pen mirrors it). Body rows run
/// `0..body`, the legs sit on row `body`.
struct Build {
    /// Total width, arms included.
    w: i32,
    body: i32,
    /// Eye columns facing front, and turned three quarters.
    eyes: [i32; 2],
    turned_eyes: [i32; 2],
    /// The arms' row.
    arm_row: i32,
    legs: [i32; 4],
    /// Columns of the shaded back side when turned.
    side: i32,
    /// The near arm when turned: a step on the front.
    turned_arm: &'static [(i32, i32)],
}

/// The rail Clawd: the Claude Code banner's, 16x5 pixels (8 cells by 2.5).
const RAIL: Build = Build {
    w: 16,
    body: 4,
    eyes: [4, 11],
    turned_eyes: [8, 12],
    arm_row: 2,
    legs: [3, 5, 10, 12],
    side: 2,
    turned_arm: &[(14, 2), (14, 3), (15, 3)],
};

/// The header Clawd, 75% of it: 12x4 pixels (6 cells by 2).
const MINI: Build = Build {
    w: 12,
    body: 3,
    eyes: [3, 8],
    turned_eyes: [6, 9],
    arm_row: 2,
    legs: [2, 4, 7, 9],
    side: 1,
    turned_arm: &[(10, 1), (10, 2), (11, 2)],
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Size {
    Rail,
    Mini,
}

impl Size {
    fn build(self) -> &'static Build {
        match self {
            Size::Rail => &RAIL,
            Size::Mini => &MINI,
        }
    }
}

/// What a Clawd is doing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Action {
    /// Facing front, blinking now and then.
    Idle,
    /// Turned three quarters toward `target`, legs striding.
    Walk,
    /// Looking aside: the near eye narrows, the near arm lifts.
    Glance,
    /// Turned three quarters, sitting, breathing.
    Sit,
    /// The mouse on its cable: swing, drop, turn and sit by it.
    Play,
    /// Up on its toes with its ears out.
    Stretch,
    /// A little jump.
    Hop,
}

#[cfg(test)]
const ACTIONS: [Action; 7] = [
    Action::Idle,
    Action::Walk,
    Action::Glance,
    Action::Sit,
    Action::Play,
    Action::Stretch,
    Action::Hop,
];

/// The surface id of the rail's napping Clawd when no agent works.
const NAPPER: SurfaceId = u64::MAX;

/// Where a header toy lives: the free cells of a pane's header row and the
/// deck rows above the pane.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HeaderStrip {
    /// The free cells of the tab bar row: the toys' floor.
    pub rect: Rect,
    /// The deck rows above the tab bar, as wide as the pane.
    pub deck: Rect,
    pub surface: SurfaceId,
    pub status: AgentStatus,
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
    /// Facing right (walking or looking that way).
    right: bool,
    /// The agent waits for input.
    idle: bool,
    /// Seconds left of a nap.
    doze: f32,
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
    size: Size,
    /// Header Clawds visit their neighbors; rail Clawds stay put.
    visits: bool,
    toys: Vec<Body>,
    last: Option<Instant>,
    rng: u64,
    clock: f32,
}

impl Default for ToyWorld {
    fn default() -> Self {
        Self {
            size: Size::Rail,
            visits: false,
            toys: Vec::new(),
            last: None,
            rng: 0x2545_F491_4F6C_DD1D,
            clock: 0.0,
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

    /// Seconds since the last step, capped so a stall does not teleport.
    fn tick(&mut self, now: Instant) -> f32 {
        let dt = self.last.map_or(0.0, |last| now.duration_since(last).as_secs_f32()).min(0.2);
        self.last = Some(now);
        self.clock += dt;
        dt
    }

    fn width(&self) -> f32 {
        self.size.build().w as f32
    }

    fn height(&self) -> f32 {
        (self.size.build().body + 1) as f32
    }

    /// Make the world hold exactly `wanted` (id, lane, lo, hi, idle). A
    /// Clawd naps a few seconds when it appears idle (or is the napper) or
    /// its agent turns idle, then wakes with a stretch.
    fn sync(&mut self, wanted: &[(SurfaceId, u16, f32, f32, bool)]) {
        self.toys.retain(|toy| wanted.iter().any(|w| w.0 == toy.id));
        for &(id, lane, lo, hi, idle) in wanted {
            let nap = self.range(NAP.0, NAP.1);
            if let Some(toy) = self.toys.iter_mut().find(|toy| toy.id == id) {
                if idle && !toy.idle {
                    toy.doze = nap;
                }
                toy.idle = idle;
                toy.home = (lo, hi);
                if toy.lane != lane || toy.visiting.is_none() {
                    toy.lane = lane;
                    toy.lo = lo;
                    toy.hi = hi;
                    toy.x = toy.x.clamp(lo, hi.max(lo));
                    toy.target = toy.target.clamp(lo, hi.max(lo));
                }
                continue;
            }
            let x = self.free_spot(lane, lo, hi);
            let toy = Body {
                id,
                lane,
                lo,
                hi,
                home: (lo, hi),
                x,
                y: 0.0,
                vx: 0.0,
                vy: 0.0,
                right: self.rand() < 0.5,
                idle,
                doze: if id == NAPPER || idle { nap } else { 0.0 },
                action: Action::Idle,
                act_t: 0.0,
                act_len: self.range(0.8, 2.0),
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
            let x = self.range(lo, hi.max(lo));
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
        let toy = &mut self.toys[i];
        if let Some((_, true)) = toy.visiting {
            // Done visiting: walk home.
            toy.start(Action::Walk, 0.0);
            toy.target = (toy.home.0 + toy.home.1) / 2.0;
            return;
        }
        let (lo, hi) = (toy.lo, toy.hi.max(toy.lo));
        match roll {
            r if r < 0.26 => toy.start(Action::Idle, 1.5 + 2.0 * a),
            r if r < 0.50 => {
                toy.start(Action::Walk, 0.0);
                let mut target = lo + (hi - lo) * a;
                if (target - toy.x).abs() < 4.0 {
                    target = if toy.x - lo > hi - toy.x { lo } else { hi };
                }
                toy.target = target;
            }
            r if r < 0.63 => {
                toy.start(Action::Glance, 1.0);
                toy.right = b < 0.5;
            }
            r if r < 0.75 => {
                toy.start(Action::Sit, 2.0 + 1.5 * a);
                toy.right = b < 0.5;
            }
            r if r < 0.86 => toy.start(Action::Play, PLAY),
            r if r < 0.93 => toy.start(Action::Stretch, 0.9),
            _ if ceiling >= 1.0 => {
                toy.start(Action::Hop, 0.0);
                toy.vy = (2.0 * GRAVITY * ceiling.min(4.0)).sqrt();
            }
            _ => toy.start(Action::Idle, 1.0 + a),
        }
    }

    /// One physics step. `ceiling` is the highest a body may hop (pixels
    /// above the floor).
    fn step(&mut self, dt: f32, ceiling: f32) {
        for i in 0..self.toys.len() {
            let toy = &mut self.toys[i];
            toy.bump_cd -= dt;
            toy.visit_cd -= dt;
            if toy.doze > 0.0 {
                toy.doze -= dt;
                toy.vx = 0.0;
                toy.visiting = None;
                if toy.doze <= 0.0 {
                    // Up again with a stretch.
                    toy.doze = 0.0;
                    toy.start(Action::Stretch, 1.0);
                }
            } else {
                toy.act_t += dt;
                let done = match toy.action {
                    Action::Walk => (toy.target - toy.x).abs() < 0.8 || toy.act_t > 8.0,
                    Action::Hop => toy.act_t > 0.15 && toy.grounded(),
                    _ => toy.act_t >= toy.act_len,
                };
                if done {
                    self.choose(i, ceiling);
                }
            }
            let toy = &mut self.toys[i];
            if toy.action == Action::Walk && !toy.asleep() {
                let dir = if toy.target > toy.x { 1.0 } else { -1.0 };
                toy.right = dir > 0.0;
                let pace = if toy.visiting.is_some() { VISIT } else { WALK };
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

    /// Clawds that bump into each other stop and look at each other; a
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
                        toy.start(Action::Glance, 1.2);
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

/// A pixel canvas. Pixels are quarter cells: two across and two down per
/// terminal cell, so `x` runs at twice the cell column.
struct Canvas {
    x0: i32,
    w: i32,
    h: i32,
    px: Vec<Option<Color>>,
    /// Text: (pixel x, pixel y, text, color).
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

/// Draws relative to a sprite's origin, mirrored when `flip` is set.
struct Pen<'a> {
    canvas: &'a mut Canvas,
    ox: i32,
    oy: i32,
    flip: bool,
    w: i32,
}

impl Pen<'_> {
    fn x(&self, dx: i32) -> i32 {
        if self.flip { self.ox + self.w - 1 - dx } else { self.ox + dx }
    }

    fn p(&mut self, dx: i32, dy: i32, color: Color) {
        let x = self.x(dx);
        self.canvas.set(x, self.oy + dy, color);
    }

    fn rows(&mut self, x0: i32, x1: i32, y0: i32, y1: i32, color: Color) {
        for dy in y0..=y1 {
            for dx in x0..=x1 {
                self.p(dx, dy, color);
            }
        }
    }

    /// Text anchored at a sprite-relative pixel; text never mirrors.
    fn say(&mut self, dx: i32, dy: i32, text: &'static str, color: Color) {
        let x = if self.flip { self.x(dx) - 2 * text.len() as i32 + 1 } else { self.x(dx) };
        self.canvas.text.push((x, self.oy + dy, text, color));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Eyes {
    Open,
    /// Mid-blink: no eyes for a frame.
    Blink,
    /// Closed, as dashes.
    Shut,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Legs {
    Stand,
    StrideA,
    StrideB,
    Tucked,
}

fn legs(pen: &mut Pen, b: &Build, legs: Legs, color: Color, shift: [i32; 4]) {
    let pick: &[usize] = match legs {
        Legs::Stand => &[0, 1, 2, 3],
        Legs::StrideA => &[0, 2],
        Legs::StrideB => &[1, 3],
        Legs::Tucked => &[],
    };
    for &k in pick {
        pen.p(b.legs[k] + shift[k], b.body, color);
    }
}

fn eye(pen: &mut Pen, x: i32, y: i32, eyes: Eyes, toward_center: i32) {
    match eyes {
        Eyes::Open => pen.p(x, y, INK),
        Eyes::Blink => {}
        Eyes::Shut => {
            pen.p(x, y, INK);
            pen.p(x + toward_center, y, INK);
        }
    }
}

/// Facing front: the banner Clawd.
fn front(pen: &mut Pen, b: &Build, eyes: Eyes, stance: Legs) {
    pen.rows(2, b.w - 3, 0, b.body - 1, ORANGE);
    pen.rows(0, b.w - 1, b.arm_row, b.arm_row, ORANGE);
    eye(pen, b.eyes[0], 1, eyes, 1);
    eye(pen, b.eyes[1], 1, eyes, -1);
    legs(pen, b, stance, ORANGE, [0; 4]);
}

/// Looking aside (right, before mirroring): the near eye narrows to a dash,
/// the near arm lifts and the far arm tucks in.
fn glance(pen: &mut Pen, b: &Build, blink: bool) {
    pen.rows(2, b.w - 3, 0, b.body - 1, ORANGE);
    pen.p(1, b.arm_row, ORANGE);
    pen.p(b.w - 2, b.arm_row - 1, ORANGE);
    pen.p(b.w - 1, b.arm_row - 1, ORANGE);
    if !blink {
        pen.p(b.eyes[0], 1, INK);
        pen.p(b.eyes[1], 1, INK);
        pen.p(b.eyes[1] + 1, 1, INK);
    }
    legs(pen, b, Legs::Stand, ORANGE, [0; 4]);
}

/// Turned three quarters to the right (before mirroring): the back side in
/// shade, the eyes toward the front, the near arm a step on the front.
fn turned(pen: &mut Pen, b: &Build, eyes: Eyes, stance: Legs, arm_low: bool) {
    pen.rows(2, b.w - 3, 0, b.body - 1, ORANGE);
    pen.rows(2, 1 + b.side, 0, b.body - 1, SIDE);
    for &(dx, dy) in b.turned_arm {
        let dy = if arm_low { (dy + 1).min(b.body - 1) } else { dy };
        pen.p(dx, dy, ORANGE);
    }
    eye(pen, b.turned_eyes[0], 1, eyes, 1);
    eye(pen, b.turned_eyes[1], 1, eyes, -1);
    // Feet point the way it faces; the back legs are in shade.
    legs(pen, b, stance, ORANGE, [1, 1, 1, 1]);
    for k in [0, 1] {
        if matches!((stance, k), (Legs::Stand, _) | (Legs::StrideA, 0) | (Legs::StrideB, 1)) {
            pen.p(b.legs[k] + 1, b.body, SIDE);
        }
    }
}

/// Up on its toes, narrower, ears out, back edge in shade.
fn stretch(pen: &mut Pen, b: &Build) {
    pen.rows(3, b.w - 4, -1, b.body - 1, ORANGE);
    pen.rows(b.w - 4, b.w - 4, -1, b.body - 1, SIDE);
    pen.p(3, -2, ORANGE);
    pen.p(4, -2, ORANGE);
    pen.p(b.w - 5, -2, ORANGE);
    pen.p(b.w - 4, -2, SIDE);
    pen.p(b.eyes[0] + 1, 0, INK);
    pen.p(b.eyes[1] - 1, 0, INK);
    legs(pen, b, Legs::Stand, ORANGE, [1, 0, 0, -1]);
}

/// The mouse on its cable, drawn behind the Clawd (negative x before
/// mirroring) at time `t` of the game.
fn cable(pen: &mut Pen, b: &Build, t: f32) {
    let arm = b.arm_row;
    let floor = b.body;
    if t < 0.4 {
        // Swung up over its head.
        for dy in -2..arm {
            pen.p(-1, dy, CORD);
        }
        pen.p(-2, -2, CORD);
        pen.p(-3, -2, CORD);
        pen.p(-4, -3, MOUSE);
        pen.p(-5, -3, MOUSE);
        pen.p(-4, -2, MOUSE);
    } else if t < 0.8 {
        // Swinging down behind it.
        pen.p(-1, arm, CORD);
        pen.p(-2, arm, CORD);
        pen.p(-3, arm, CORD);
        pen.p(-4, arm - 1, MOUSE);
        pen.p(-5, arm - 1, MOUSE);
        pen.p(-5, arm, MOUSE);
        pen.p(-6, arm - 1, MOUSE);
    } else {
        // On the floor, the cable running back to it.
        let mut x = -1;
        let mut y = arm + 1;
        while y < floor {
            pen.p(x, y, CORD);
            x -= 1;
            y += 1;
        }
        for dx in x - 2..=x {
            pen.p(dx, floor, CORD);
        }
        pen.p(x - 3, floor, MOUSE);
        pen.p(x - 4, floor, MOUSE);
    }
}

/// Draw one Clawd. `ground` is the canvas row its feet stand on.
fn draw_toy(canvas: &mut Canvas, toy: &Body, b: &Build, x_off: i32, ground: i32, clock: f32) {
    let frame = ((clock + toy.phase) * 10.0) as u32;
    let blink = frame % 38 == 0;
    let eyes = if blink { Eyes::Blink } else { Eyes::Open };
    let mut oy = ground - b.body - toy.y.round() as i32;
    let ox = x_off + toy.x.round() as i32;
    let flip = !toy.right;
    if toy.asleep() {
        // Settled on its belly, eyes shut.
        oy += 1;
        let mut pen = Pen { canvas, ox, oy, flip, w: b.w };
        front(&mut pen, b, Eyes::Shut, Legs::Tucked);
        let z = ((clock * 1.2) as i32) % 3;
        pen.say(b.w + 1 + 2 * z, -1 - z, if z == 2 { "Z" } else { "z" }, SKY);
        return;
    }
    if !toy.grounded() {
        let mut pen = Pen { canvas, ox, oy, flip, w: b.w };
        front(&mut pen, b, eyes, Legs::Tucked);
        return;
    }
    let t = toy.act_t;
    match toy.action {
        Action::Idle | Action::Hop => {
            let mut pen = Pen { canvas, ox, oy, flip, w: b.w };
            front(&mut pen, b, eyes, Legs::Stand);
        }
        Action::Walk => {
            let stance = if toy.vx.abs() > 0.5 {
                if (frame / 2) % 2 == 0 { Legs::StrideA } else { Legs::StrideB }
            } else {
                Legs::Stand
            };
            let mut pen = Pen { canvas, ox, oy, flip, w: b.w };
            turned(&mut pen, b, eyes, stance, false);
        }
        Action::Glance => {
            let mut pen = Pen { canvas, ox, oy, flip, w: b.w };
            glance(&mut pen, b, blink);
        }
        Action::Sit => {
            let breathe = ((t * 1.6) as i32) % 2 == 1;
            let mut pen = Pen { canvas, ox, oy, flip, w: b.w };
            turned(&mut pen, b, eyes, Legs::Stand, breathe);
        }
        Action::Stretch => {
            let mut pen = Pen { canvas, ox, oy, flip, w: b.w };
            if t < 0.15 || t > toy.act_len - 0.15 {
                front(&mut pen, b, eyes, Legs::Stand);
            } else {
                stretch(&mut pen, b);
            }
        }
        Action::Play => {
            {
                let mut pen = Pen { canvas: &mut *canvas, ox, oy, flip, w: b.w };
                cable(&mut pen, b, t);
            }
            // Once the mouse is down it turns to face it and sits by it.
            let mut pen = Pen { canvas, ox, oy, flip: if t >= 1.1 { !flip } else { flip }, w: b.w };
            if t < 1.1 || t > PLAY - 0.3 {
                front(&mut pen, b, eyes, Legs::Stand);
            } else {
                let breathe = ((t * 1.6) as i32) % 2 == 1;
                turned(&mut pen, b, eyes, Legs::Stand, breathe);
            }
        }
    }
}

/// The quarter-block glyph for a mask: bit 0 top left, 1 top right, 2
/// bottom left, 3 bottom right.
const QUADRANTS: [&str; 16] =
    [" ", "▘", "▝", "▀", "▖", "▌", "▞", "▛", "▗", "▚", "▐", "▜", "▄", "▙", "▟", "█"];

fn distance(a: Color, b: Color) -> i32 {
    match (a, b) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let d = |x: u8, y: u8| (i32::from(x) - i32::from(y)).pow(2);
            d(r1, r2) + d(g1, g2) + d(b1, b2)
        }
        _ => i32::MAX,
    }
}

/// A cell's four pixels as (glyph, fg, bg): at most two colors, as a
/// terminal cell shows one glyph in a foreground and a background. Extra
/// colors fall to the nearest kept one; an empty pixel shows `under`.
fn quantize(pixels: [Option<Color>; 4], under: Color) -> Option<(&'static str, Color, Color)> {
    let mut counts: Vec<(Color, usize)> = Vec::new();
    for color in pixels.iter().flatten() {
        match counts.iter_mut().find(|(c, _)| c == color) {
            Some((_, n)) => *n += 1,
            None => counts.push((*color, 1)),
        }
    }
    if counts.is_empty() {
        return None;
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    let empty = pixels.iter().any(Option::is_none);
    let keep = if empty { 1 } else { 2.min(counts.len()) };
    let kept: Vec<Color> = counts.iter().take(keep).map(|(c, _)| *c).collect();
    let snap = |color: Color| *kept.iter().min_by_key(|k| distance(**k, color)).unwrap();
    let fg = kept[0];
    let bg = if empty { under } else { *kept.get(1).unwrap_or(&fg) };
    let mut mask = 0;
    for (bit, pixel) in pixels.iter().enumerate() {
        if pixel.is_some_and(|color| snap(color) == fg) {
            mask |= 1 << bit;
        }
    }
    Some((QUADRANTS[mask], fg, bg))
}

/// Quarter-block render of `canvas` into `area`; `allow` limits which
/// cells (absolute x, y) may change.
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
            let (x, y) = (2 * i32::from(ax), 2 * i32::from(cy));
            let pixels = [
                canvas.get(x, y),
                canvas.get(x + 1, y),
                canvas.get(x, y + 1),
                canvas.get(x + 1, y + 1),
            ];
            let cell = &mut buf[(ax, ay)];
            let under = bg.unwrap_or(cell.bg);
            if let Some((glyph, fg, back)) = quantize(pixels, under) {
                cell.set_symbol(glyph).set_style(Style::default().fg(fg).bg(back));
            }
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
        let start = x.div_euclid(2).clamp(lo, (hi - len).max(lo));
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

/// Whether `area` is big enough to hold a rail toy.
pub(crate) fn fits(area: Rect) -> bool {
    area.width >= MIN_WIDTH && area.height >= MIN_HEIGHT
}

/// Step the rail world and draw it into `area`. `agents` lists every
/// agent and its status; working ones get a Clawd, else one Clawd naps.
pub(crate) fn draw_rail(
    world: &mut ToyWorld,
    buf: &mut Buffer,
    area: Rect,
    agents: &[(SurfaceId, AgentStatus)],
    bg: Color,
    dim: Style,
) {
    if !fits(area) {
        return;
    }
    world.size = Size::Rail;
    world.visits = false;
    let b = &RAIL;
    let w = 2 * i32::from(area.width);
    let h = 2 * i32::from(area.height);
    let (lo, hi) = (2.0, (w - b.w - 2).max(2) as f32);
    // As many Clawds as the floor holds side by side.
    let room = ((w - 2) / (b.w + 1)).max(1) as usize;
    let mut ids: Vec<SurfaceId> = agents
        .iter()
        .filter(|agent| agent.1 == AgentStatus::Working)
        .map(|agent| agent.0)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let mut wanted: Vec<_> =
        ids.into_iter().take(MAX_TOYS.min(room)).map(|id| (id, 0u16, lo, hi, false)).collect();
    if wanted.is_empty() {
        // Nobody works: one Clawd naps a few seconds, then wakes up.
        wanted.push((NAPPER, 0, lo, hi, false));
    }
    // Room overhead for ears and the cable swung over the head.
    let ceiling = (h - (b.body + 1) - 4).clamp(0, 4) as f32;
    world.sync(&wanted);
    let dt = world.tick(Instant::now());
    world.step(dt, ceiling);
    let x0 = 2 * i32::from(area.x);
    let mut canvas = Canvas::new(x0, w, h);
    let ground = h - 1;
    for toy in &world.toys {
        draw_toy(&mut canvas, toy, b, x0, ground, world.clock);
    }
    blit(buf, area, &canvas, Some(bg), dim, &|_, _| true);
}

/// Step the header world and draw a small Clawd for every agent pane: it
/// lives on the free cells of the tab bar row and in the deck rows above,
/// and walks over to visit Clawds of neighbor panes on the same row.
/// Returns whether any toy was drawn.
pub(crate) fn draw_header(
    world: &mut ToyWorld,
    buf: &mut Buffer,
    strips: &[HeaderStrip],
    _agents: &[(SurfaceId, AgentStatus)],
    dim: Style,
) -> bool {
    world.size = Size::Mini;
    world.visits = true;
    let b = &MINI;
    let strips: Vec<_> =
        strips.iter().filter(|strip| i32::from(strip.rect.width) * 2 >= b.w + 6).copied().collect();
    let wanted: Vec<_> = strips
        .iter()
        .map(|strip| {
            let lo = 2.0 * f32::from(strip.rect.x) + 2.0;
            let hi = 2.0 * f32::from(strip.rect.x + strip.rect.width) - b.w as f32 - 4.0;
            let idle = strip.status == AgentStatus::Idle;
            (strip.surface, strip.rect.y, lo, hi.max(lo), idle)
        })
        .collect();
    // Room overhead for the ears and the cable: the deck minus the body.
    let ceiling = (DECK_PX - (b.body + 1) - 2).max(0) as f32;
    world.sync(&wanted);
    if world.toys.is_empty() {
        return false;
    }
    let dt = world.tick(Instant::now());
    world.step(dt, ceiling);
    let mut rows: Vec<u16> = strips.iter().map(|strip| strip.rect.y).collect();
    rows.sort_unstable();
    rows.dedup();
    let screen = *buf.area();
    let deck_rows = crate::app::TOY_DECK_ROWS;
    for row in rows {
        let Some(top) = row.checked_sub(deck_rows) else { continue };
        let mut canvas = Canvas::new(0, 2 * i32::from(screen.x + screen.width), DECK_PX);
        let ground = DECK_PX - 1;
        let mut visitor_span: Vec<(u16, u16)> = Vec::new();
        for toy in world.toys.iter().filter(|toy| toy.lane == row) {
            draw_toy(&mut canvas, toy, b, 0, ground, world.clock);
            if toy.visiting.is_some() {
                let a = (toy.x.round() as i32).div_euclid(2) - 1;
                let z = (toy.x.round() as i32 + b.w).div_euclid(2) + 2;
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

    /// The front pose is the Claude Code banner's Clawd, pixel for pixel.
    #[test]
    fn toy_front_pose_is_the_banner_clawd() {
        let mut canvas = Canvas::new(0, 16, 5);
        let mut pen = Pen { canvas: &mut canvas, ox: 0, oy: 0, flip: false, w: 16 };
        front(&mut pen, &RAIL, Eyes::Open, Legs::Stand);
        let art: String = (0..5)
            .map(|y| {
                (0..16)
                    .map(|x| match canvas.get(x, y) {
                        Some(INK) => 'o',
                        Some(_) => '#',
                        None => '.',
                    })
                    .collect::<String>()
                    + "\n"
            })
            .collect();
        assert_eq!(
            art,
            "..############..\n\
             ..##o######o##..\n\
             ################\n\
             ..############..\n\
             ...#.#....#.#...\n"
        );
    }

    #[test]
    fn toy_does_every_move_and_stays_on_its_floor() {
        let mut world = ToyWorld::default();
        world.sync(&[(1, 0, 0.0, 60.0, false)]);
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
        for action in ACTIONS {
            assert!(seen.contains(&action), "never did {action:?}; did {seen:?}");
        }
    }

    #[test]
    fn toy_bodies_never_overlap_on_the_rail_floor() {
        let mut world = ToyWorld::default();
        let wanted: Vec<_> = (0..4u64).map(|id| (id, 0, 0.0, 90.0, false)).collect();
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
        let mut world = ToyWorld { size: Size::Mini, visits: true, ..ToyWorld::default() };
        let wanted = [(1, 0, 0.0, 20.0, false), (2, 0, 60.0, 80.0, false)];
        world.sync(&wanted);
        let mut visited = false;
        for _ in 0..3000 {
            world.clock += 0.08;
            world.step(0.08, 1.0);
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
        let area = Rect { x: 0, y: 0, width: 20, height: 8 };
        let mut buf = Buffer::empty(ratatui::layout::Rect::new(0, 0, 20, 8));
        let mut world = ToyWorld::default();
        let agents = [(1, AgentStatus::Idle)];
        let mut frames = 0;
        let tick = |world: &mut ToyWorld, buf: &mut Buffer| {
            draw_rail(world, buf, area, &agents, Color::Reset, Style::default());
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
        let area = Rect { x: 2, y: 3, width: 40, height: 12 };
        let mut buf = Buffer::empty(ratatui::layout::Rect::new(0, 0, 50, 20));
        let mut world = ToyWorld::default();
        let agents = [(1, AgentStatus::Working), (2, AgentStatus::Working), (3, AgentStatus::Idle)];
        for _ in 0..200 {
            draw_rail(&mut world, &mut buf, area, &agents, Color::Reset, Style::default());
            world.rewind_for_test(Duration::from_millis(80));
        }
        assert_eq!(world.toys.len(), 2);
        for y in 0..20 {
            for x in 0..50 {
                let inside = (2..42).contains(&x) && (3..15).contains(&y);
                if !inside {
                    assert_eq!(buf[(x, y)].symbol(), " ", "drew outside at {x},{y}");
                }
            }
        }
    }

    /// A Clawd mid-move for the sprite dump.
    fn posed(action: Action, t: f32, right: bool) -> Body {
        Body {
            id: 1,
            lane: 0,
            lo: 0.0,
            hi: 99.0,
            home: (0.0, 99.0),
            x: 10.0,
            y: if action == Action::Hop { 2.0 } else { 0.0 },
            vx: if action == Action::Walk { 5.0 } else { 0.0 },
            vy: 0.0,
            right,
            idle: false,
            doze: 0.0,
            action,
            act_t: t,
            act_len: if action == Action::Stretch { 0.9 } else { PLAY },
            target: 10.0,
            bump_cd: 0.0,
            visit_cd: 0.0,
            visiting: None,
            phase: 0.0,
        }
    }

    /// The pixels a terminal shows for `buf`: each cell's quarter-block
    /// glyph in its foreground over its background.
    fn shown(buf: &Buffer, bg: Color) -> Vec<Vec<Color>> {
        let area = *buf.area();
        let mut rows = vec![vec![bg; usize::from(area.width) * 2]; usize::from(area.height) * 2];
        for y in 0..area.height {
            for x in 0..area.width {
                let cell = &buf[(x, y)];
                let mask = QUADRANTS.iter().position(|q| *q == cell.symbol()).unwrap_or(0);
                for bit in 0..4 {
                    let on = mask & (1 << bit) != 0;
                    let color = if on {
                        cell.fg
                    } else if mask == 0 {
                        bg
                    } else {
                        cell.bg
                    };
                    let (px, py) = (usize::from(x) * 2 + bit % 2, usize::from(y) * 2 + bit / 2);
                    rows[py][px] = match color {
                        Color::Reset => bg,
                        other => other,
                    };
                }
            }
        }
        rows
    }

    /// `CMUX_TOY_DUMP=<file> cargo test toy_dump -- --ignored` writes every
    /// pose at both sizes as the terminal would show them.
    #[test]
    #[ignore]
    fn toy_dump_frames() {
        let Ok(path) = std::env::var("CMUX_TOY_DUMP") else { return };
        let bg = Color::Rgb(0x10, 0x16, 0x2A);
        let poses: [(Action, f32, bool, bool); 12] = [
            (Action::Idle, 0.5, true, false),
            (Action::Idle, 0.5, true, true),
            (Action::Glance, 0.5, true, false),
            (Action::Glance, 0.5, false, false),
            (Action::Walk, 0.5, true, false),
            (Action::Walk, 0.7, false, false),
            (Action::Sit, 0.2, false, false),
            (Action::Stretch, 0.5, true, false),
            (Action::Play, 0.2, true, false),
            (Action::Play, 0.6, true, false),
            (Action::Play, 0.9, true, false),
            (Action::Play, 1.5, true, false),
        ];
        let mut out = String::new();
        for (name, b) in [("rail", &RAIL), ("mini", &MINI)] {
            for (n, &(action, t, right, asleep)) in poses.iter().enumerate() {
                let (cols, rows) = (18u16, 5u16);
                let mut canvas = Canvas::new(0, i32::from(cols) * 2, i32::from(rows) * 2);
                let mut toy = posed(action, t, right);
                if asleep {
                    toy.doze = 1.0;
                }
                draw_toy(&mut canvas, &toy, b, 0, i32::from(rows) * 2 - 1, 0.35 * n as f32);
                let mut buf = Buffer::empty(ratatui::layout::Rect::new(0, 0, cols, rows));
                let area = Rect { x: 0, y: 0, width: cols, height: rows };
                blit(&mut buf, area, &canvas, Some(bg), Style::default(), &|_, _| true);
                out.push_str(&format!("{name} {n}\n"));
                for row in shown(&buf, bg) {
                    for color in row {
                        match color {
                            Color::Rgb(r, g, b) => out.push_str(&format!("{r:02x}{g:02x}{b:02x} ")),
                            _ => out.push_str("...... "),
                        }
                    }
                    out.push('\n');
                }
            }
        }
        std::fs::write(path, out).unwrap();
    }
}
