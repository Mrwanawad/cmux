# Checkpoint: cmux-tui personal increments

**Version: 0.8.0** (2026-11-27)

Branch `windows-wsl`, pushed to the fork `Mrwanawad/cmux` (remote `fork`,
public, forked from `manaflow-ai/cmux`, where this account cannot push).
`origin` stays the upstream for syncing. The running cmux uses
`target/release/cmux-tui`; to load a new build, close the cmux window and run
`cmux` again (the background daemon keeps the sessions). Outside the repo,
`~/.claude/settings.json` in WSL has `"remoteControlAtStartup": true`.

## Build and test

- Release build (WSL): `cargo build --release -p cmux-tui` (about 8 min).
- Focused tests (dev profile, fast):
  `cargo test -p cmux-tui --bin cmux-tui -- toy_ session_topic shift_enter plain_pane`
- Full suite: `cargo test -p cmux-tui --bin cmux-tui` (about 1950 tests).
- Sprite sheet for eyeballing: `CMUX_TOY_DUMP=<file> cargo test -p cmux-tui
  --bin cmux-tui toy_dump -- --ignored`, then turn the dump into a PNG
  (a short zlib/struct script; no PIL on this machine).

## Version history

### 0.8.0: real-pixel 3D Clawds over Sixel
- The user asked for 3D bots and said the icons looked shifted. Root cause of
  every earlier rejection: a 2-row deck is 8x6 half-block pixels. Windows
  Terminal 1.25 supports Sixel (DA1 has `4`) with a fixed 10x20 px cell
  (`CSI 16 t` -> `6;20;10`), so a 2-row deck is 40 px tall.
- New `ui/toy3d.rs`: a voxel Clawd (body 14x10x10, stub arms, four legs, eye
  slits), posed per frame (`Pose3`: yaw, leg lift, low, tall, arms, eyes,
  flat), turned about its vertical axis, low camera (20 deg), per-face light
  in 5 steps, 1px outline, floor shadow, Sixel encoder (1:1 aspect, every
  pixel set). Deck Clawds at 2 px/voxel, rail at 3 (2.2 when short).
- `toy.rs`: `pose3()` maps each action to a 3D pose (spin is a real 360 deg
  turn; other turns ease), `draw_rail_sixel`/`draw_header_sixel` push one
  image per deck / rail area to `ToyWorld::jobs` and blank those cells.
  Outfits and tools are not drawn in 3D (plain Clawd).
- `app.rs::draw_terminal`: after the cell frame, drops images whose cells
  something else drew over (menus, prompts), writes the rest
  (`ESC 7`, CUP, DCS sixel, `ESC 8`), sends an image only when its bytes or
  place changed, repaints all cells (`terminal.clear`) the frame after a toy
  area disappears, and resends after a resize. Never draws on the last
  screen row (Sixel there scrolls the host).
- On when `WT_SESSION` is set; `CMUX_TOYS_SIXEL=1/0` forces it. Off in tests.
- Eyeball: `CMUX_TOY3D_DUMP=<ppm> CMUX_TOY3D_SIXEL=<six> cargo test -p
  cmux-tui --bin cmux-tui toy3d_dump -- --ignored`; `cat` the .six in a WT
  tab to see the real thing.
- Verified: 3 new `toy3d_` tests plus the 17 toy tests green; the dumped
  Sixel frames render crisp in a Windows Terminal tab (right size, colors,
  1:1 aspect). Not yet seen inside a live cmux (needs a cmux restart).
- Docs: `docs/configuration.md` (toys paragraph, `CMUX_TOYS_SIXEL`).
- Skills installed for this (Windows `~/.claude/skills`, symlinked into
  WSL): `sprite-stacking` (danielwh2) and `pixel-art-studio` (Gamezxz). Run
  their scripts with Windows `python.exe` (WSL python has no Pillow).

### 0.7.0: small shaded Clawds with outfits, tools and more moves (superseded by 0.8.0)
- The user found the flat pixel Clawds (half-block, 10x7, 20x14 on the rail,
  3-row deck) "way too big and too vague". Rewritten in `toy.rs`:
  - **Size:** 8x6 square half-cell pixels everywhere (rail too, no 2x);
    the deck is back to 2 rows (`TOY_DECK_ROWS`).
  - **3D shading:** lit top face (back corner rounded), shaded side face,
    dark underside, far arm and far legs darker. Three views: 3/4 side,
    front (idle, cheer) and back (spin).
  - **Outfits** (`Outfit`, random per Clawd, new one mid-spin after every
    nap and sometimes on a random spin): fedora with band, cap with bill,
    beanie, headphones, crown, tennis headband; scarf with a flapping tail,
    bow tie, gold chain; shades with a glint. Muted cloth colors (navy,
    burgundy, bottle green, camel, stone, charcoal, oxford blue).
  - **Tools** read from the agent screen (`⏺ Read(...)`, `⏺ Bash(...)`):
    magnifier, pen, hard hat and hammer, bug net, rocket, cloud, suit and
    briefcase, beret and brush, wrench. Tool hats/suits replace the outfit.
  - **Moves:** walk with arm swing and stride, look, sit, stretch, hop
    with a landing squash, dance (bob and turn on the beat), spin, wave
    (blocked agent, or two Clawds meeting), cheer when the agent finishes,
    then a 3-5 s nap.
- Tests: `toy_front_pose_is_a_shaded_block`,
  `toy_side_pose_turns_its_shade_away`, `toy_outfits_stay_on_the_body`,
  every move incl. Land/Dance/Spin, tool detection; full suite 1962 green.
- **Verdict:** the user saw it live and said it looks bad ("looking like
  shit"). Saved as a record only; the next attempt should start from a new
  visual direction, not tweak this sprite.

### 0.6.0: Clawd cloned from the Claude desktop app
- The user rated 0.5.0 2.5/10 ("very strange") and gave a screen recording
  of the Claude desktop app's mascot (`Desktop\sdsd.mp4`, 4.7 s). Frames
  were cut with Windows ffmpeg and studied at 6x. `toy.rs` was rewritten
  to clone it:
  - **Sprite:** the Claude Code banner Clawd (`▐▛███▜▌ / ▝▜█████▛▘ /
    ▘▘ ▝▝`), flat Claude orange, two eye notches, side arms, four thin
    legs. Rail 16x5 quarter-cell pixels (8 cells by 2.5), header 12x4 (6
    cells by 2). `toy_front_pose_is_the_banner_clawd` pins it pixel for
    pixel.
  - **Moves (from the video):** idle with blinks; glance aside (near eye a
    dash, near arm up, far arm tucked); turn three quarters with the back
    side shaded to walk (striding legs, feet forward) or sit (breathing);
    stretch up with ears out; hop; and the mouse game: swing the mouse
    over its head on its cable, drop it behind, turn and sit by it.
  - **Behaviour:** an action picker per Clawd (`Action`, `choose`);
    bumping Clawds stop and look at each other; header Clawds walk over to
    visit a neighbor pane's Clawd and walk home; a nap of 3-5 s when a
    Clawd appears idle or its agent turns idle, ending in a stretch.
  - **Removed:** personas, hats, glasses, props, per-agent colors, speech
    bubbles, sparks, flashes, shadows. Every Clawd is Claude orange, as in
    the reference. Only the nap keeps its small "z".
- Tests: `toy_front_pose_is_the_banner_clawd`,
  `toy_does_every_move_and_stays_on_its_floor`, overlap, visitor goes home,
  napper wakes, rail draw stays inside, plus the end-to-end deck tests.

### 0.5.0: 3D block toys, redrawn from scratch
- The user found 0.4.0's toys muddy ("shit designs"). The drawing layer was
  rewritten after the 3D Clawd reel and the Claude Code banner:
  - **Quarter-block renderer.** Pixels are quarter cells (`▘▝▀▖▌▞▛▗▚▐▜▄▙▟█`),
    twice the horizontal detail of half blocks. `quantize` keeps at most two
    colors per cell (fg and bg), snapping extras to the nearest kept color.
    On screen a pixel is about twice as tall as wide.
  - **A real 3D block.** `clawd()` draws a parametric block (`Build`): lit top
    face with a rounded back corner, front face with a rim light, darker side
    face, darker bevel underneath, stub arms (far one in shade), and four
    block legs (back pair in shade) with a stride. Eyes are tall slits
    toward the front (three quarters, never at the viewer); a content
    squint when happy; lids when napping.
  - **Rail Clawd 14x7 px** (7 cells by 3.5), **header Clawd 10x5 px** (70%).
  - **One clean accessory per persona** instead of full-body outfits:
    Gentleman navy fedora with a red band and gold watch, Golfer beige cap
    and club, Photographer camera at the hip and up to the eye with a
    one-frame flash, Executive beige cap and coffee, Tennis white sweatband
    and racket, Mentor beige cap and chalkboard, Connoisseur two-lens
    sunglasses and perfume.
  - Removed the header speed lines and header landing dust (stray grey
    pixels). Speeds doubled in pixels so on-screen speed is unchanged.
- The sprite dump (`toy_dump_frames`) now renders through the real
  quarter-block renderer and rebuilds the shown pixels from the glyphs, so
  the preview PNG matches the terminal.

### 0.4.0: toy deck, 3/4 view, session names, Shift+Enter, Remote Control
- **Session names on tab chips.** An agent tab chip shows the session's topic
  (`0 CMUX animated sleeping toys`) instead of `0 claude`. Claude Code titles
  its terminal after the conversation; `sidebar_status::session_topic` strips
  the spinner glyph, skips bare program names ("claude", "Claude Code") and
  shortens to 28 chars with `…` (`TOPIC_WIDTH` in `ui/pane.rs`). A tab the
  user renamed keeps its name.
- **Toy deck above agent panes.** Every pane whose active tab runs an agent
  gives `TOY_DECK_ROWS` (2) rows above its tab bar to a deck (`app.rs`:
  `toy_deck_panes`, `toy_deck_trim`, `toy_deck_fits`; threaded through
  `PaneAreaProjection.toy_decks`). The pane's terminal shrinks by exactly
  those rows through the normal resize path. Panes shorter than 8 rows,
  stacked headers and surface-only mode get no deck. `"sidebar": {"toys":
  false}` removes the decks and the rows come back.
- **Header toys at 70% size**, drawn with the sidebar open or closed. A
  5x4 px mini Clawd (2-row head, outfit row, shoes) walks the free cells of
  the tab bar row, jumps a full row into the deck, and its speech, z's,
  camera flash, golf ball, tennis ball and chalkboard play out in the deck.
  It dashes to a neighbor pane's toy on the same row and comes home.
- **Idle means a short nap, not sleep.** A toy whose agent turns idle naps
  3-5 s (`NAP`), then wakes with "08:57!", a sparkle poof and a hop.
- **3/4 view and block-toy look** (superseded by 0.5.0) (reference: the 3D Clawd reel). Toys face
  about 50 degrees toward where they walk: eyes and collar sit toward the
  front, the far eye is softer, the back column and far arm are shaded, the
  front edge and top are lit, and rail toys have a bevel (darker underside).
  Nobody looks at the viewer.
- **Depth and motion.** Rail toys cast a soft floor shadow that shrinks as
  they rise; landings kick up a small dust puff.
- **Shift+Enter is a newline in agent panes.** cmux sends ESC CR (what
  Claude Code's `/terminal-setup` installs) for a bare Shift+Enter when the
  active tab runs an agent (`agent_newline_bytes`). Shells keep Shift+Enter
  as Enter.
- **Remote Control at startup.** `~/.claude/settings.json` in WSL now has
  `"remoteControlAtStartup": true`, matching the Windows settings. Claude
  sessions started in cmux connect to Remote Control right away.
- Tests added: `app::tests::toy_deck` (end to end through a real mux and
  the TestBackend: deck rows, toys drawn with the sidebar open and closed,
  rows returned when toys are off, plain panes untouched, Shift+Enter
  bytes), `session_topic_names_agent_tabs_after_the_conversation`.

### 0.3.0: wardrobe redesign (Stockholm style)
- Replaced the persona set (no guns, Santa or mobster). Seven dressed
  personas: Gentleman (navy suit, red tie, gold watch), Golfer (beige cap,
  polo, swing and ball flight), Photographer (old camera, one-frame flash,
  toys in front say "*pose*"), Executive (beige cap, gilet, coffee), Tennis
  (whites, racket, ball), Mentor (senior engineer: cap, camel gilet,
  chalkboard), Connoisseur (leather jacket, Byredo/Amouage spray).
- Colors: Claude orange, royal navy, camel, bottle green, burgundy, stone,
  oxford blue. Collisions say "cheers", "*handshake*", "pardon!".
- Rail body grew to 7x5 px (2 head rows, 2 outfit rows, legs).
- With no agent working, the rail napper is a Gentleman that sleeps 3-5 s
  after it appears, then wakes dressed.

### 0.2.0: header toys and physics
- Shared physics world (`ToyWorld`): gravity, jumps, walls, pounces,
  collisions with sparks and speech bubbles, sort-and-sweep `separate()` so
  bodies never overlap.
- With the sidebar closed, agent panes got a 1-row mini toy after the tab
  chips (replaced by the deck in 0.4.0).

### 0.1.0: Clawd toys in the sidebar
- Small pixel-art Clawds in the left sidebar's empty space, below the
  workspace rows. One toy per working agent; with none, one Clawd naps.
- Half-block rendering (2 px per cell), 80 ms frames (`toy::FRAME`) only
  while a toy is visible (`sidebar_toy_wait`).
- `config.rs`: `sidebar.toys` (default true).

## Files
- `crates/cmux-tui/src/ui/toy.rs`: personas, sprites, physics, rail and
  header renderers (block and Sixel), `pose3()` action-to-3D mapping, toy
  tests.
- `crates/cmux-tui/src/ui/toy3d.rs`: voxel Clawd model, 3D rasterizer,
  Sixel encoder and `emit`, `toy3d_` tests and the `toy3d_dump` preview.
- `crates/cmux-tui/src/ui/sidebar.rs`: `draw_toys()` for the rail.
- `crates/cmux-tui/src/ui/pane.rs`: topic chips, `HeaderStrip` with deck.
- `crates/cmux-tui/src/ui/mod.rs`: draws header toys every frame.
- `crates/cmux-tui/src/app.rs`: deck layout, `toy_decks`, Shift+Enter, toy
  frame pacing.
- `crates/cmux-tui/src/sidebar_status.rs`: `session_topic`.
- `crates/cmux-tui/src/app/tests/toy_deck.rs`: end-to-end tests.

## Known limits
- 3D Clawds need a Sixel host; detection is `WT_SESSION` only (Windows
  Terminal 1.22+). Other hosts get the 0.7.0 block Clawds unless
  `CMUX_TOYS_SIXEL=1`. A cmux nested inside a cmux pane inherits
  `WT_SESSION` and would send Sixel to the inner terminal.
- Sixel assumes Windows Terminal's fixed 10x20 px cell; a host with another
  Sixel cell size would scale the images wrong.
- Outfits and tool props are not drawn in 3D yet.
- Deck Clawds stand in the 2 deck rows, not on the tab bar row (a Sixel
  image is a rectangle and would cover the tab chips).
- Block mode: a Clawd is 8x6 half-cell pixels, too small for detail.
- Shift+Enter needs the host terminal to report Shift with Enter (cmux asks
  for the kitty keyboard protocol); in a host that does not, it stays Enter.

## Next steps
- Get the user's verdict on 0.8.0 live (close the cmux window, run `cmux`).
  0.5.0-0.7.0 were rejected; if 0.8.0 lands, commit and push to the fork.
- If wanted: outfits and tool props as voxel parts in `toy3d::build`.
- Look at it live: open 4-5 Claude panes, check deck spacing, chip names
  and toy visits across panes.
- Open a PR upstream from `Mrwanawad/cmux:windows-wsl` if the user wants
  it in manaflow-ai/cmux.
