use serde::Deserialize;
use std::env;
use std::fs;
use std::path::PathBuf;
use winit::event::VirtualKeyCode;

#[derive(Clone, Copy, Debug)]
pub struct KeyBindings {
    pub a: VirtualKeyCode,
    pub b: VirtualKeyCode,
    pub select: VirtualKeyCode,
    pub start: VirtualKeyCode,
    pub right: VirtualKeyCode,
    pub left: VirtualKeyCode,
    pub up: VirtualKeyCode,
    pub down: VirtualKeyCode,
    pub r: VirtualKeyCode,
    pub l: VirtualKeyCode,
    pub save_state: VirtualKeyCode,
    pub load_state: VirtualKeyCode,
}

#[derive(Default, Deserialize)]
struct ConfigFile {
    keybinds: Option<KeybindOverrides>,
}

#[derive(Default, Deserialize)]
struct KeybindOverrides {
    a: Option<String>,
    b: Option<String>,
    select: Option<String>,
    start: Option<String>,
    right: Option<String>,
    left: Option<String>,
    up: Option<String>,
    down: Option<String>,
    r: Option<String>,
    l: Option<String>,
    save_state: Option<String>,
    load_state: Option<String>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            a: VirtualKeyCode::Z,
            b: VirtualKeyCode::X,
            select: VirtualKeyCode::Back,
            start: VirtualKeyCode::Return,
            right: VirtualKeyCode::Right,
            left: VirtualKeyCode::Left,
            up: VirtualKeyCode::Up,
            down: VirtualKeyCode::Down,
            r: VirtualKeyCode::S,
            l: VirtualKeyCode::A,
            save_state: VirtualKeyCode::F5,
            load_state: VirtualKeyCode::F8,
        }
    }
}

impl KeyBindings {
    pub fn load() -> Self {
        let Some(path) = default_config_path() else {
            return Self::default();
        };

        if !path.exists() {
            return Self::default();
        }

        let Ok(bytes) = fs::read(&path) else {
            return Self::default();
        };
        println!("Loading config from {}", path.display());
        let Ok(config) = serde_json::from_slice::<ConfigFile>(&bytes) else {
            eprintln!("Failed to parse config {}", path.display());
            return Self::default();
        };

        let mut bindings = Self::default();
        if let Some(overrides) = config.keybinds {
            apply_override(&mut bindings.a, overrides.a.as_deref(), "a", &path);
            apply_override(&mut bindings.b, overrides.b.as_deref(), "b", &path);
            apply_override(
                &mut bindings.select,
                overrides.select.as_deref(),
                "select",
                &path,
            );
            apply_override(
                &mut bindings.start,
                overrides.start.as_deref(),
                "start",
                &path,
            );
            apply_override(
                &mut bindings.right,
                overrides.right.as_deref(),
                "right",
                &path,
            );
            apply_override(&mut bindings.left, overrides.left.as_deref(), "left", &path);
            apply_override(&mut bindings.up, overrides.up.as_deref(), "up", &path);
            apply_override(&mut bindings.down, overrides.down.as_deref(), "down", &path);
            apply_override(&mut bindings.r, overrides.r.as_deref(), "r", &path);
            apply_override(&mut bindings.l, overrides.l.as_deref(), "l", &path);
            apply_override(
                &mut bindings.save_state,
                overrides.save_state.as_deref(),
                "save_state",
                &path,
            );
            apply_override(
                &mut bindings.load_state,
                overrides.load_state.as_deref(),
                "load_state",
                &path,
            );
        }

        bindings
    }

    pub fn button_bit(self, key: VirtualKeyCode) -> Option<u8> {
        match key {
            k if k == self.a => Some(0),
            k if k == self.b => Some(1),
            k if k == self.select => Some(2),
            k if k == self.start => Some(3),
            k if k == self.right => Some(4),
            k if k == self.left => Some(5),
            k if k == self.up => Some(6),
            k if k == self.down => Some(7),
            k if k == self.r => Some(8),
            k if k == self.l => Some(9),
            _ => None,
        }
    }
}

fn default_config_path() -> Option<PathBuf> {
    let home = env::var_os("HOME")?;
    let mut path = PathBuf::from(home);
    path.push(".config");
    path.push("vibe-gba");
    path.push("config.json");
    Some(path)
}

fn apply_override(slot: &mut VirtualKeyCode, raw: Option<&str>, field: &str, path: &PathBuf) {
    let Some(raw) = raw else {
        return;
    };
    match parse_key(raw) {
        Some(key) => *slot = key,
        None => eprintln!(
            "Ignoring invalid key '{}' for {} in {}",
            raw,
            field,
            path.display()
        ),
    }
}

fn parse_key(raw: &str) -> Option<VirtualKeyCode> {
    let key = raw.trim().to_ascii_uppercase();
    Some(match key.as_str() {
        "A" => VirtualKeyCode::A,
        "B" => VirtualKeyCode::B,
        "C" => VirtualKeyCode::C,
        "D" => VirtualKeyCode::D,
        "E" => VirtualKeyCode::E,
        "F" => VirtualKeyCode::F,
        "G" => VirtualKeyCode::G,
        "H" => VirtualKeyCode::H,
        "I" => VirtualKeyCode::I,
        "J" => VirtualKeyCode::J,
        "K" => VirtualKeyCode::K,
        "L" => VirtualKeyCode::L,
        "M" => VirtualKeyCode::M,
        "N" => VirtualKeyCode::N,
        "O" => VirtualKeyCode::O,
        "P" => VirtualKeyCode::P,
        "Q" => VirtualKeyCode::Q,
        "R" => VirtualKeyCode::R,
        "S" => VirtualKeyCode::S,
        "T" => VirtualKeyCode::T,
        "U" => VirtualKeyCode::U,
        "V" => VirtualKeyCode::V,
        "W" => VirtualKeyCode::W,
        "X" => VirtualKeyCode::X,
        "Y" => VirtualKeyCode::Y,
        "Z" => VirtualKeyCode::Z,
        "UP" => VirtualKeyCode::Up,
        "DOWN" => VirtualKeyCode::Down,
        "LEFT" => VirtualKeyCode::Left,
        "RIGHT" => VirtualKeyCode::Right,
        "ENTER" | "RETURN" => VirtualKeyCode::Return,
        "BACK" | "BACKSPACE" => VirtualKeyCode::Back,
        "SPACE" => VirtualKeyCode::Space,
        "LSHIFT" | "LEFTSHIFT" => VirtualKeyCode::LShift,
        "RSHIFT" | "RIGHTSHIFT" => VirtualKeyCode::RShift,
        "LCTRL" | "LEFTCTRL" | "LCONTROL" | "LEFTCONTROL" => VirtualKeyCode::LControl,
        "RCTRL" | "RIGHTCTRL" | "RCONTROL" | "RIGHTCONTROL" => VirtualKeyCode::RControl,
        "F1" => VirtualKeyCode::F1,
        "F2" => VirtualKeyCode::F2,
        "F3" => VirtualKeyCode::F3,
        "F4" => VirtualKeyCode::F4,
        "F5" => VirtualKeyCode::F5,
        "F6" => VirtualKeyCode::F6,
        "F7" => VirtualKeyCode::F7,
        "F8" => VirtualKeyCode::F8,
        "F9" => VirtualKeyCode::F9,
        "F10" => VirtualKeyCode::F10,
        "F11" => VirtualKeyCode::F11,
        "F12" => VirtualKeyCode::F12,
        _ => return None,
    })
}
