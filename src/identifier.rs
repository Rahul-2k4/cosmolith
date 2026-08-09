use std::env;

#[allow(dead_code)]
#[derive(Debug)]
// The following is just an intermediatry to be passed to Compsoitor Module
// compositor::init_compositor will match and convert the identified compositor to
// their equivalent structs
pub enum Desktop {
    Hyprland,
    Sway,
    Gnome,
    Kde,
    Plasma,
    Xfce,
    Cosmic,
    Wayland,
    X11,
    Tty,
    Unknown(String),
}

#[test]
fn empty_environment_values_are_not_session_indicators() {
    let _guard = EnvGuard::clear();
    for name in [
        "XDG_SESSION_TYPE",
        "HYPRLAND_INSTANCE_SIGNATURE",
        "SWAYSOCK",
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_DESKTOP",
        "DESKTOP_SESSION",
        "WAYLAND_DISPLAY",
        "DISPLAY",
    ] {
        unsafe { env::set_var(name, " ") };
    }

    assert!(matches!(get_current_session(), Desktop::Unknown(_)));
}

#[test]
fn cosmic_sway_desktop_value_selects_sway_backend() {
    let _guard = EnvGuard::clear();
    unsafe { env::set_var("XDG_CURRENT_DESKTOP", "Regolith-Wayland:COSMIC:sway") };

    assert!(matches!(get_current_session(), Desktop::Sway));
}

struct EnvGuard {
    values: Vec<(&'static str, Option<String>)>,
}

impl EnvGuard {
    fn clear() -> Self {
        let names = [
            "XDG_SESSION_TYPE",
            "HYPRLAND_INSTANCE_SIGNATURE",
            "SWAYSOCK",
            "XDG_CURRENT_DESKTOP",
            "XDG_SESSION_DESKTOP",
            "DESKTOP_SESSION",
            "WAYLAND_DISPLAY",
            "DISPLAY",
        ];
        let values = names
            .into_iter()
            .map(|name| {
                let previous = env::var(name).ok();
                unsafe { env::remove_var(name) };
                (name, previous)
            })
            .collect();
        Self { values }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (name, value) in self.values.drain(..) {
            match value {
                Some(value) => unsafe { env::set_var(name, value) },
                None => unsafe { env::remove_var(name) },
            }
        }
    }
}

// #todo : Find edge cases where this logic might fail?
// Think of other ways the following can be made more robust :}
pub fn get_current_session() -> Desktop {
    if let Some(session_type) = non_empty_env("XDG_SESSION_TYPE") {
        match session_type.to_lowercase().as_str() {
            "tty" => return Desktop::Tty,
            "wayland" => {}
            "x11" => {}
            _ => {}
        }
    }

    if non_empty_env("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
        return Desktop::Hyprland;
    }
    if non_empty_env("SWAYSOCK").is_some() {
        return Desktop::Sway;
    }
    let candidates = [
        non_empty_env("XDG_CURRENT_DESKTOP"),
        non_empty_env("XDG_SESSION_DESKTOP"),
        non_empty_env("DESKTOP_SESSION"),
    ];

    for value in candidates.into_iter().flatten() {
        let lower = value.to_lowercase();
        if lower.contains("cosmic") && lower.contains("sway") {
            return Desktop::Sway;
        }
        if lower.contains("hyprland") {
            return Desktop::Hyprland;
        }
        if lower.contains("sway") {
            return Desktop::Sway;
        }
        if lower.contains("gnome") {
            return Desktop::Gnome;
        }
        if lower.contains("kde") {
            return Desktop::Kde;
        }
        if lower.contains("plasma") {
            return Desktop::Plasma;
        }
        if lower.contains("xfce") {
            return Desktop::Xfce;
        }
        if lower.contains("cosmic") {
            return Desktop::Cosmic;
        }
    }

    if non_empty_env("WAYLAND_DISPLAY").is_some() {
        return Desktop::Wayland;
    }
    if non_empty_env("DISPLAY").is_some() {
        return Desktop::X11;
    }

    Desktop::Unknown("Not Detected".into())
}

fn non_empty_env(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}
