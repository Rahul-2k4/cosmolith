use std::env;

#[allow(dead_code)]
#[derive(Debug, PartialEq, Eq)]
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

#[derive(Debug, Default, Clone, Copy)]
pub struct SessionEnvironment<'a> {
    pub session_type: Option<&'a str>,
    pub hyprland_signature: Option<&'a str>,
    pub sway_socket: Option<&'a str>,
    pub current_desktop: Option<&'a str>,
    pub session_desktop: Option<&'a str>,
    pub desktop_session: Option<&'a str>,
    pub wayland_display: Option<&'a str>,
    pub display: Option<&'a str>,
}

pub fn detect_session(environment: SessionEnvironment<'_>) -> Desktop {
    if environment
        .session_type
        .is_some_and(|value| value.eq_ignore_ascii_case("tty"))
    {
        return Desktop::Tty;
    }

    if environment.hyprland_signature.is_some() {
        return Desktop::Hyprland;
    }

    let candidates = [
        environment.current_desktop,
        environment.session_desktop,
        environment.desktop_session,
    ];

    // COSMIC commonly runs on the Sway-compatible Regolith session. Preserve
    // that identity for callers while dispatching it to Sway below.
    if candidates
        .iter()
        .flatten()
        .any(|value| value.to_lowercase().contains("cosmic"))
    {
        return Desktop::Cosmic;
    }

    if environment.sway_socket.is_some() {
        return Desktop::Sway;
    }

    for value in candidates.into_iter().flatten() {
        let lower = value.to_lowercase();
        if lower.contains("hyprland") {
            return Desktop::Hyprland;
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
        if lower.contains("sway") {
            return Desktop::Sway;
        }
    }

    if environment.wayland_display.is_some() {
        return Desktop::Wayland;
    }
    if environment.display.is_some() {
        return Desktop::X11;
    }

    Desktop::Unknown("Not Detected".into())
}

#[cfg(test)]
mod tests {
    use super::{Desktop, SessionEnvironment, detect_session};

    #[test]
    fn detects_cosmic_over_sway_from_current_desktop() {
        let session = detect_session(SessionEnvironment {
            session_type: Some("wayland"),
            sway_socket: Some("/run/user/1000/sway-ipc.sock"),
            current_desktop: Some("Regolith-Wayland:COSMIC:sway"),
            ..Default::default()
        });

        assert_eq!(session, Desktop::Cosmic);
    }

    #[test]
    fn falls_back_to_sway_when_gnome_desktop_has_sway_socket() {
        let session = detect_session(SessionEnvironment {
            session_type: Some("wayland"),
            sway_socket: Some("/run/user/1000/sway-ipc.sock"),
            current_desktop: Some("GNOME"),
            ..Default::default()
        });

        assert_eq!(session, Desktop::Sway);
    }

    #[test]
    fn returns_unknown_without_session_or_display_environment() {
        let session = detect_session(SessionEnvironment::default());

        assert_eq!(session, Desktop::Unknown("Not Detected".into()));
    }
}

// #todo : Find edge cases where this logic might fail?
// Think of other ways the following can be made more robust :}
pub fn get_current_session() -> Desktop {
    let session_type = env::var("XDG_SESSION_TYPE").ok();
    let hyprland_signature = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();
    let sway_socket = env::var("SWAYSOCK").ok();
    let current_desktop = env::var("XDG_CURRENT_DESKTOP").ok();
    let session_desktop = env::var("XDG_SESSION_DESKTOP").ok();
    let desktop_session = env::var("DESKTOP_SESSION").ok();
    let wayland_display = env::var("WAYLAND_DISPLAY").ok();
    let display = env::var("DISPLAY").ok();

    detect_session(SessionEnvironment {
        session_type: session_type.as_deref(),
        hyprland_signature: hyprland_signature.as_deref(),
        sway_socket: sway_socket.as_deref(),
        current_desktop: current_desktop.as_deref(),
        session_desktop: session_desktop.as_deref(),
        desktop_session: desktop_session.as_deref(),
        wayland_display: wayland_display.as_deref(),
        display: display.as_deref(),
    })
}
