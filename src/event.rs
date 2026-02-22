use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[derive(Clone)]
pub enum Action {
    Quit,
    MoveUp,
    MoveDown,
    CollapseOrParent,
    ExpandOrChild,
    Toggle,
    Select,
    Kill,
    ConfirmKill,
    CancelKill,
    Refresh,
    EnterFilter,
    FilterChar(char),
    FilterBackspace,
    FilterDeleteForward,
    FilterKillWord,
    FilterKillLine,
    FilterKillLineForward,
    FilterCursorLeft,
    FilterCursorRight,
    FilterCursorWordLeft,
    FilterCursorWordRight,
    FilterCursorStart,
    FilterCursorEnd,
    ExitFilter,
    None,
}

#[derive(Clone, PartialEq)]
pub enum Mode {
    Normal,
    Confirming,
    Filtering,
}

pub fn map_key(key: KeyEvent, mode: &Mode) -> Action {
    if key.kind != KeyEventKind::Press {
        return Action::None;
    }

    match mode {
        Mode::Normal => match key.code {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char('k') | KeyCode::Up => Action::MoveUp,
            KeyCode::Char('j') | KeyCode::Down => Action::MoveDown,
            KeyCode::Char('h') | KeyCode::Left => Action::CollapseOrParent,
            KeyCode::Char('l') | KeyCode::Right => Action::ExpandOrChild,
            KeyCode::Char(' ') => Action::Toggle,
            KeyCode::Enter => Action::Select,
            KeyCode::Char('x') => Action::Kill,
            KeyCode::Char('r') => Action::Refresh,
            KeyCode::Char('/') => Action::EnterFilter,
            _ => Action::None,
        },
        Mode::Confirming => match key.code {
            KeyCode::Char('y') => Action::ConfirmKill,
            KeyCode::Char('n') | KeyCode::Esc => Action::CancelKill,
            _ => Action::None,
        },
        Mode::Filtering => match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => Action::ExitFilter,
            (KeyCode::Enter, _) => Action::Select,
            (KeyCode::Up, _) => Action::MoveUp,
            (KeyCode::Down, _) => Action::MoveDown,
            (KeyCode::Char('a'), KeyModifiers::CONTROL) => Action::FilterCursorStart,
            (KeyCode::Char('e'), KeyModifiers::CONTROL) => Action::FilterCursorEnd,
            (KeyCode::Char('u'), KeyModifiers::CONTROL) | (KeyCode::Backspace, KeyModifiers::SUPER) => Action::FilterKillLine,
            (KeyCode::Char('k'), KeyModifiers::CONTROL) => Action::FilterKillLineForward,
            (KeyCode::Char('b'), KeyModifiers::CONTROL) | (KeyCode::Left, KeyModifiers::NONE) => Action::FilterCursorLeft,
            (KeyCode::Char('f'), KeyModifiers::CONTROL) | (KeyCode::Right, KeyModifiers::NONE) => Action::FilterCursorRight,
            (KeyCode::Left, KeyModifiers::ALT) => Action::FilterCursorWordLeft,
            (KeyCode::Right, KeyModifiers::ALT) => Action::FilterCursorWordRight,
            (KeyCode::Backspace, KeyModifiers::ALT) => Action::FilterKillWord,
            (KeyCode::Backspace, _) => Action::FilterBackspace,
            (KeyCode::Delete, _) => Action::FilterDeleteForward,
            (KeyCode::Char(c), _) if c.is_ascii_graphic() || c == ' ' => Action::FilterChar(c),
            _ => Action::None,
        },
    }
}
