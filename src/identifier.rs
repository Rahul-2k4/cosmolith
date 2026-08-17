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

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    use super::{env, get_current_session, Desktop};

    #[test]
    fn empty_environment_values_are_not_session_indicators() {
        let _lock = environment_test_lock();
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
            unsafe { env::set_var(name, "") };
        }

        assert!(matches!(get_current_session(), Desktop::Unknown(_)));
    }

    #[test]
    fn whitespace_environment_values_are_not_session_indicators() {
        let _lock = environment_test_lock();
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
        let _lock = environment_test_lock();
        let _guard = EnvGuard::clear();
        unsafe { env::set_var("XDG_CURRENT_DESKTOP", "Regolith-Wayland:COSMIC:sway") };

        assert!(matches!(get_current_session(), Desktop::Sway));
    }

    #[test]
    fn desktop_tokens_match_sway_without_substring_false_positives() {
        let _lock = environment_test_lock();
        let _guard = EnvGuard::clear();

        unsafe { env::set_var("XDG_CURRENT_DESKTOP", "Not-Sway") };
        assert!(matches!(get_current_session(), Desktop::Unknown(_)));

        unsafe { env::set_var("XDG_CURRENT_DESKTOP", "Sway") };
        assert!(matches!(get_current_session(), Desktop::Sway));

        unsafe { env::set_var("XDG_CURRENT_DESKTOP", "COSMIC:Sway") };
        assert!(matches!(get_current_session(), Desktop::Sway));
    }

    fn environment_test_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
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
        if has_desktop_token(&value, "cosmic") && has_desktop_token(&value, "sway") {
            return Desktop::Sway;
        }
        if has_desktop_token(&value, "hyprland") {
            return Desktop::Hyprland;
        }
        if has_desktop_token(&value, "sway") {
            return Desktop::Sway;
        }
        if has_desktop_token(&value, "gnome") {
            return Desktop::Gnome;
        }
        if has_desktop_token(&value, "kde") {
            return Desktop::Kde;
        }
        if has_desktop_token(&value, "plasma") {
            return Desktop::Plasma;
        }
        if has_desktop_token(&value, "xfce") {
            return Desktop::Xfce;
        }
        if has_desktop_token(&value, "cosmic") {
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

fn has_desktop_token(value: &str, expected: &str) -> bool {
    value
        .split(':')
        .any(|token| token.eq_ignore_ascii_case(expected))
}

fn non_empty_env(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}
