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
        Mode::Normal => match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) => Action::Quit,
            (KeyCode::Char('k'), _) | (KeyCode::Up, _) => Action::MoveUp,
            (KeyCode::Char('p'), KeyModifiers::CONTROL) => Action::MoveUp,
            (KeyCode::Char('j'), _) | (KeyCode::Down, _) => Action::MoveDown,
            (KeyCode::Char('n'), KeyModifiers::CONTROL) => Action::MoveDown,
            (KeyCode::Char('h'), _) | (KeyCode::Left, _) => Action::CollapseOrParent,
            (KeyCode::Char('l'), _) | (KeyCode::Right, _) => Action::ExpandOrChild,
            (KeyCode::Char(' '), _) => Action::Toggle,
            (KeyCode::Enter, _) => Action::Select,
            (KeyCode::Char('x'), _) => Action::Kill,
            (KeyCode::Char('r'), _) => Action::Refresh,
            (KeyCode::Char('/'), _) => Action::EnterFilter,
            _ => Action::None,
        },
        Mode::Confirming => match key.code {
            KeyCode::Enter => Action::ConfirmKill,
            KeyCode::Esc => Action::CancelKill,
            _ => Action::None,
        },
        Mode::Filtering => match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => Action::ExitFilter,
            (KeyCode::Enter, _) => Action::Select,
            (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => Action::MoveUp,
            (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => Action::MoveDown,
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
