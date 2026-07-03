use std::io;

pub fn pins_path() -> io::Result<String> {
    let home = std::env::var("HOME")
        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "$HOME not set"))?;
    Ok(format!("{}/.config/tmux-sessions/pins.json", home))
}

pub fn load_pins() -> Vec<String> {
    let path = match pins_path() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    match serde_json::from_str::<Vec<String>>(&content) {
        Ok(v) => v,
        Err(e) => {
            let ts = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                Ok(d) => d.as_secs(),
                Err(_) => return Vec::new(),
            };
            let backup = format!("{}.broken.{}", path, ts);
            let _ = std::fs::rename(&path, &backup);
            eprintln!("tmux-sessions: pins.json was corrupt ({e}); moved to {backup}");
            Vec::new()
        }
    }
}

pub fn save_pins(pinned: &[String]) {
    let path = match pins_path() {
        Ok(p) => p,
        Err(_) => return,
    };
    if let Ok(json) = serde_json::to_string(pinned) {
        let _ = std::fs::write(&path, json);
    }
}

pub fn hidden_path() -> io::Result<String> {
    let home = std::env::var("HOME")
        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "$HOME not set"))?;
    Ok(format!("{}/.config/tmux-sessions/hidden.json", home))
}

pub fn load_hidden() -> Vec<String> {
    let path = match hidden_path() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    match serde_json::from_str::<Vec<String>>(&content) {
        Ok(v) => v,
        Err(e) => {
            let ts = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                Ok(d) => d.as_secs(),
                Err(_) => return Vec::new(),
            };
            let backup = format!("{}.broken.{}", path, ts);
            let _ = std::fs::rename(&path, &backup);
            eprintln!("tmux-sessions: hidden.json was corrupt ({e}); moved to {backup}");
            Vec::new()
        }
    }
}

pub fn save_hidden(hidden: &[String]) {
    let path = match hidden_path() {
        Ok(p) => p,
        Err(_) => return,
    };
    if let Ok(json) = serde_json::to_string(hidden) {
        let _ = std::fs::write(&path, json);
    }
}
