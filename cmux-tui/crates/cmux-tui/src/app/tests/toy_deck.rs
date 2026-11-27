//! Header toys end to end: an agent pane gives two rows above its tab bar
//! to a deck where a mini Clawd plays, with the sidebar open or closed; a
//! plain pane keeps every row; Shift+Enter in an agent pane is a newline.

use std::time::{Duration, Instant};

use super::*;

const EVENT_WAIT: Duration = Duration::from_secs(10);
const SIZE: (u16, u16) = (100, 24);

fn draw(app: &mut App, terminal: &mut Terminal<TestBackend>) {
    app.sync_layout(SIZE);
    terminal.draw(|frame| crate::ui::draw(app, frame)).unwrap();
}

fn agent_app(name: &str) -> (Arc<Mux>, Arc<cmux_tui_core::Surface>, App, Terminal<TestBackend>) {
    let (mux, tab) = test_mux(name, None);
    mux.report_agent(tab.id, AgentState::Working, AgentSource::Hook, Some("s1".into())).unwrap();
    let mut app = test_app(Session::Local(mux.clone()));
    app.sidebar_view = SidebarView::Workspaces;
    app.replace_tree(app.session.tree());
    let mut terminal = Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).unwrap();
    let deadline = Instant::now() + EVENT_WAIT;
    while app.sidebar_status.tab(tab.id).is_none() {
        assert!(Instant::now() < deadline, "the agent never reached the sidebar status");
        draw(&mut app, &mut terminal);
        std::thread::sleep(Duration::from_millis(10));
    }
    draw(&mut app, &mut terminal);
    (mux, tab, app, terminal)
}

fn area_of(app: &App, surface: SurfaceId) -> PaneArea {
    *app.pane_areas.iter().find(|area| area.surface == surface).expect("the agent pane is laid out")
}

/// Cells of `rows` inside `x0..x1` drawn with a block glyph, the toys' pixels.
fn toy_cells(
    terminal: &Terminal<TestBackend>,
    x0: u16,
    x1: u16,
    rows: std::ops::Range<u16>,
) -> usize {
    let buffer = terminal.backend().buffer();
    rows.flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            buffer[(x, y)]
                .symbol()
                .chars()
                .next()
                .is_some_and(|c| ('\u{2580}'..='\u{259F}').contains(&c))
        })
        .count()
}

/// Draws frames for `seconds` of toy time, `step` apart.
fn play(app: &mut App, terminal: &mut Terminal<TestBackend>, seconds: f32) -> usize {
    let mut most = 0;
    let frames = (seconds / 0.08) as usize;
    for _ in 0..frames {
        app.toy_header.rewind_for_test(Duration::from_millis(80));
        draw(app, terminal);
        let area = area_of(app, app.active_surface().unwrap());
        let top = area.rect.y.saturating_sub(crate::app::TOY_DECK_ROWS);
        most = most.max(toy_cells(
            terminal,
            area.rect.x,
            area.rect.x + area.rect.width,
            top..area.rect.y + 1,
        ));
    }
    most
}

#[test]
fn an_agent_pane_gets_a_toy_deck_with_the_sidebar_open_or_closed() {
    let (mux, tab, mut app, mut terminal) = agent_app("toy-deck-e2e-test");
    assert!(app.toy_decks.contains(&area_of(&app, tab.id).pane), "the agent pane has a deck");
    let with_deck = area_of(&app, tab.id);
    let bar = with_deck.bar.expect("the agent pane has a tab bar");
    assert_eq!(bar.y, with_deck.rect.y, "the tab bar tops the trimmed pane rect");
    assert!(bar.y >= crate::app::TOY_DECK_ROWS, "the deck rows sit above the tab bar");

    // The toy shows up, naps a few seconds, wakes and keeps moving.
    let pixels = play(&mut app, &mut terminal, 6.0);
    assert!(pixels >= 3, "a mini Clawd is drawn in the deck or on the tab bar ({pixels} cells)");
    assert!(!app.header_toy_strips.is_empty(), "the pane header offers its free cells");

    // The sidebar closed: the toy still plays.
    app.sidebar_visible = false;
    let pixels = play(&mut app, &mut terminal, 1.0);
    assert!(pixels >= 3, "the toy plays with the sidebar closed too ({pixels} cells)");
    app.sidebar_visible = true;

    // Toys off: the pane takes its two rows back and its terminal grows.
    app.config.sidebar.toys = false;
    draw(&mut app, &mut terminal);
    let without = area_of(&app, tab.id);
    assert!(app.toy_decks.is_empty());
    assert_eq!(
        without.content.height,
        with_deck.content.height + crate::app::TOY_DECK_ROWS,
        "the deck costs exactly its rows"
    );
    mux.close_surface(tab.id).unwrap();
}

#[test]
fn a_plain_pane_keeps_every_row() {
    let (mux, tab) = test_mux("toy-deck-plain-pane-test", None);
    let mut app = test_app(Session::Local(mux.clone()));
    app.replace_tree(app.session.tree());
    let mut terminal = Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).unwrap();
    draw(&mut app, &mut terminal);
    assert!(app.toy_decks.is_empty(), "no agent, no deck");
    assert!(app.header_toy_strips.is_empty());
    mux.close_surface(tab.id).unwrap();
}

#[test]
fn shift_enter_is_a_newline_only_in_agent_panes() {
    let shift_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT);
    assert_eq!(crate::app::agent_newline_bytes(&shift_enter, true), Some(b"\x1b\r".as_slice()));
    assert_eq!(
        crate::app::agent_newline_bytes(&shift_enter, false),
        None,
        "a shell still runs the line"
    );
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(crate::app::agent_newline_bytes(&enter, true), None, "Enter still sends the prompt");
    let ctrl_shift = KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT | KeyModifiers::CONTROL);
    assert_eq!(crate::app::agent_newline_bytes(&ctrl_shift, true), None);
}
