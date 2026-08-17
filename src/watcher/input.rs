// Watch Input Config Changes

use std::{error::Error as StdError, sync::mpsc::Sender};

use cosmic_comp_config::input::InputConfig;
use cosmic_comp_config::{KeyboardConfig, XkbConfig};
use cosmic_config::{Config, ConfigGet};

use crate::error::Error;
use crate::event::{
    input::{KeyboardEvent, MouseEvent, TouchpadEvent},
    Event,
};
use std::sync::{Arc, Mutex};

// #todo : Find all the keys linked to  com.system76.CosmicComp and catch those and read events
// implemented
// 1. input_touchpad
// 2. input_default
// 3. xkb_config
// 4. keyboard_config
// to be implemented
// 5. workspaces
// 6. pinned_workspaces
// 7. input_touchpad_override
// 8. input_devices
// 9. autotile
// 10. autotile_behaviour
// 11. active_hint
// 12. focus_follows_cursor
// 13. cursor_follows_focus
// 14. focus_follows_cursor_delay
// 15. descale_xwayland
// 16. xwayland_eavesdropping
// 17. edge_snap_threshold
// 18. accessbility_zoom

pub const INPUTNAMESPACE: &str = "com.system76.CosmicComp";
pub const VERSION: u64 = 1;

pub struct InputState {
    touchpad: Option<InputConfig>,
    mouse: Option<InputConfig>,
    // #todo: Find which exact type is used to emit and monitor changes for this
    // Add that here and then
    // 1. pattern match / 2. add events / 3. impl from() / 4. Events -> Ipc Calls Mapping
    keyboard: Option<XkbConfig>,
    numslock: Option<KeyboardConfig>,
}

fn startup_keyboard_events(config: XkbConfig) -> Vec<Event> {
    KeyboardEvent::from(XkbConfig::default(), config)
}

fn send_events(
    tx: &Arc<Mutex<Sender<Event>>>,
    events: Vec<Event>,
) -> Result<(), Box<dyn StdError>> {
    let sender = tx.lock().map_err(|_| Error::channel_lock("input"))?;
    for event in events {
        sender
            .send(event)
            .map_err(|source| Error::channel_send("input", source))?;
    }

    Ok(())
}

pub fn send_initial_input_events(tx: &Arc<Mutex<Sender<Event>>>) -> Result<(), Box<dyn StdError>> {
    let config = Config::new(INPUTNAMESPACE, VERSION)
        .map_err(|source| Error::config_init(INPUTNAMESPACE, source))?;

    match config.get::<XkbConfig>("xkb_config") {
        Ok(current_keyboard) => send_events(tx, startup_keyboard_events(current_keyboard))?,
        Err(source) => eprintln!(
            "{}",
            Error::config_read(INPUTNAMESPACE, "xkb_config", source)
        ),
    }

    Ok(())
}

pub fn start_input_watcher(
    tx: &Arc<Mutex<Sender<Event>>>,
) -> Result<Box<dyn std::any::Any + Send>, Box<dyn StdError>> {
    let config = Config::new(INPUTNAMESPACE, VERSION)
        .map_err(|source| Error::config_init(INPUTNAMESPACE, source))?;
    let state = Arc::new(Mutex::new(InputState {
        touchpad: config.get::<InputConfig>("input_touchpad").ok(),
        mouse: config.get::<InputConfig>("input_default").ok(),
        keyboard: config.get::<XkbConfig>("xkb_config").ok(),
        numslock: config.get::<KeyboardConfig>("keyboard_config").ok(),
    }));

    // Keep the watcher alive for the lifetime of the program.
    let watcher = config
        .watch({
            let tx = Arc::clone(&tx);
            let state = Arc::clone(&state);
            move |cfg: &Config, keys| match tx.lock() {
                Ok(sender) => match state.lock() {
                    Ok(mut state) => {
                        for event in state.from(cfg, keys) {
                            if let Err(source) = sender.send(event) {
                                eprintln!("{}", Error::channel_send("input", source));
                            }
                        }
                    }
                    Err(_) => eprintln!("{}", Error::channel_lock("input state")),
                },
                Err(_) => eprintln!("{}", Error::channel_lock("input")),
            }
        })
        .map_err(|source| Error::watcher_setup("input", source))?;

    Ok(Box::new(watcher))
}

impl InputState {
    pub fn from(&mut self, cfg: &Config, keys: &[String]) -> Vec<Event> {
        let mut events = Vec::new();
        for key in keys {
            match key.as_str() {
                "input_touchpad" => match cfg.get::<InputConfig>(key) {
                    Ok(new_config) => {
                        if let Some(old) = self.touchpad.clone() {
                            events.extend(TouchpadEvent::from(old, new_config.clone()));
                        }
                        self.touchpad = Some(new_config);
                    }
                    Err(source) => eprintln!("{}", Error::config_read(INPUTNAMESPACE, key, source)),
                },
                "input_default" => match cfg.get::<InputConfig>(key) {
                    Ok(new_config) => {
                        if let Some(old) = self.mouse.clone() {
                            events.extend(MouseEvent::from(old, new_config.clone()));
                        }
                        self.mouse = Some(new_config);
                    }
                    Err(source) => eprintln!("{}", Error::config_read(INPUTNAMESPACE, key, source)),
                },
                "xkb_config" => match cfg.get::<XkbConfig>(key) {
                    Ok(new_config) => {
                        let old = self.keyboard.clone().unwrap_or_default();
                        events.extend(KeyboardEvent::from(old, new_config.clone()));
                        self.keyboard = Some(new_config);
                    }
                    Err(source) => eprintln!("{}", Error::config_read(INPUTNAMESPACE, key, source)),
                },
                "keyboard_config" => match cfg.get::<KeyboardConfig>(key) {
                    Ok(new_config) => {
                        if let Some(old) = self.numslock.clone() {
                            events.extend(KeyboardEvent::from_keyboard_config(
                                old,
                                new_config.clone(),
                            ));
                        }
                        self.numslock = Some(new_config);
                    }
                    Err(source) => eprintln!("{}", Error::config_read(INPUTNAMESPACE, key, source)),
                },
                x => {
                    eprintln!(
                        "Unknown key found in Input (com.system76.CosmicComp): {}",
                        x
                    );
                }
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::{startup_keyboard_events, InputState};
    use crate::event::{
        input::{InputEvent, KeyboardEvent},
        Event,
    };
    use cosmic_comp_config::XkbConfig;
    use cosmic_config::{Config, ConfigSet};

    fn config_with_xkb(xkb: XkbConfig) -> Config {
        static NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("cosmolith-input-test-{}-{id}", std::process::id()));
        let config = Config::with_custom_path("com.system76.CosmicComp", 1, path).unwrap();
        config.set("xkb_config", xkb).unwrap();
        config
    }

    fn keyboard_events(events: &[Event]) -> Vec<&KeyboardEvent> {
        events
            .iter()
            .filter_map(|event| match event {
                Event::Input(InputEvent::Keyboard(event)) => Some(event),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn input_watcher_reads_single_layout_from_xkb_config() {
        let config = config_with_xkb(XkbConfig {
            layout: "us".into(),
            ..XkbConfig::default()
        });
        let mut state = InputState {
            touchpad: None,
            mouse: None,
            keyboard: None,
            numslock: None,
        };
        let events = state.from(&config, &["xkb_config".into()]);
        assert!(keyboard_events(&events).contains(&&KeyboardEvent::Layout("us".into())));
    }

    #[test]
    fn input_watcher_emits_variant_only_change() {
        let config = config_with_xkb(XkbConfig {
            layout: "us".into(),
            variant: "intl".into(),
            ..XkbConfig::default()
        });
        let mut state = InputState {
            touchpad: None,
            mouse: None,
            keyboard: Some(XkbConfig {
                layout: "us".into(),
                ..XkbConfig::default()
            }),
            numslock: None,
        };
        let events = state.from(&config, &["xkb_config".into()]);
        assert_eq!(
            keyboard_events(&events),
            vec![&KeyboardEvent::Variant("intl".into())]
        );
    }

    #[test]
    fn input_watcher_preserves_multi_layout_and_variant_values() {
        let config = config_with_xkb(XkbConfig {
            layout: "us,fr".into(),
            variant: "intl,oss".into(),
            ..XkbConfig::default()
        });
        let mut state = InputState {
            touchpad: None,
            mouse: None,
            keyboard: None,
            numslock: None,
        };
        let events = state.from(&config, &["xkb_config".into()]);
        assert_eq!(
            keyboard_events(&events),
            vec![
                &KeyboardEvent::Layout("us,fr".into()),
                &KeyboardEvent::Variant("intl,oss".into()),
            ]
        );
    }

    #[test]
    fn keyboard_event_conversion_emits_variant_only_change() {
        let old = XkbConfig {
            layout: "us".into(),
            ..XkbConfig::default()
        };
        let new = XkbConfig {
            layout: "us".into(),
            variant: "intl".into(),
            ..old.clone()
        };

        let events = KeyboardEvent::from(old, new);

        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            Event::Input(InputEvent::Keyboard(KeyboardEvent::Variant(variant)))
                if variant == "intl"
        ));
    }

    #[test]
    fn startup_keyboard_events_emit_multi_layout_and_matching_variants() {
        let config = XkbConfig {
            layout: "us,fr".into(),
            variant: "intl,oss".into(),
            ..XkbConfig::default()
        };

        let events = startup_keyboard_events(config);

        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            Event::Input(InputEvent::Keyboard(KeyboardEvent::Layout(layout)))
                if layout == "us,fr"
        ));
        assert!(matches!(
            &events[1],
            Event::Input(InputEvent::Keyboard(KeyboardEvent::Variant(variant)))
                if variant == "intl,oss"
        ));
    }
}
