use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

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
        Mode::Filtering => match key.code {
            KeyCode::Esc => Action::ExitFilter,
            KeyCode::Enter => Action::Select,
            KeyCode::Char('j') | KeyCode::Down => Action::MoveDown,
            KeyCode::Char('k') | KeyCode::Up => Action::MoveUp,
            KeyCode::Backspace => Action::FilterBackspace,
            KeyCode::Char(c) if c.is_ascii_graphic() || c == ' ' => Action::FilterChar(c),
            _ => Action::None,
        },
    }
}
