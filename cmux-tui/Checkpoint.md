# Checkpoint: cmux-tui personal increments

**Version: 0.6.0** (2026-11-27)

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
  header renderers, toy tests.
- `crates/cmux-tui/src/ui/sidebar.rs`: `draw_toys()` for the rail.
- `crates/cmux-tui/src/ui/pane.rs`: topic chips, `HeaderStrip` with deck.
- `crates/cmux-tui/src/ui/mod.rs`: draws header toys every frame.
- `crates/cmux-tui/src/app.rs`: deck layout, `toy_decks`, Shift+Enter, toy
  frame pacing.
- `crates/cmux-tui/src/sidebar_status.rs`: `session_topic`.
- `crates/cmux-tui/src/app/tests/toy_deck.rs`: end-to-end tests.

## Known limits
- Terminal cells cap the detail: a rail Clawd is 16x5 quarter-cell pixels, a
  header Clawd 12x4. A quarter-cell pixel is twice as tall as wide, so the
  eyes are tall notches, as in the Claude Code banner.
- Quarter blocks need the host terminal to draw block elements itself
  (Windows Terminal does; so does the Claude Code banner).
- The camera flash lasts one 80 ms frame, the shortest a frame can be.
- Shift+Enter needs the host terminal to report Shift with Enter (cmux asks
  for the kitty keyboard protocol); in a host that does not, it stays Enter.

## Next steps
- Look at it live: open 4-5 Claude panes, check deck spacing, chip names
  and toy visits across panes.
- Open a PR upstream from `Mrwanawad/cmux:windows-wsl` if the user wants
  it in manaflow-ai/cmux.
