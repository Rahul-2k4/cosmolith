// src/main.rs
use cosmic_config::Config;
use std::{
    error::Error as StdError,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

mod watcher;
mod error;
use watcher::input::{send_initial_input_events, start_input_watcher};
use crate::error::Error;
mod event;
use event::Event;

mod identifier;
use identifier::get_current_session;

mod compositor;
use compositor::init_compositor;

mod persistence;
mod display_persistence;

use watcher::shortcuts::start_shortcuts_watcher;

fn main() -> Result<(), Box<dyn StdError>> {
    let _config = Config::new("com.system76.CosmicComp", 1)
        .map_err(|source| Error::config_init("com.system76.CosmicComp", source))?;
    // Channel used to receive change notifications from the watcher callback.
    let (tx, rx) = mpsc::channel::<Event>();
    let tx = Arc::new(Mutex::new(tx));

    let _watcher = start_input_watcher(&tx)?;
    let _shortcuts_watcher = start_shortcuts_watcher(&tx)?;
    send_initial_input_events(&tx)?;

    println!("Watching for configuration changes…");

    let session = get_current_session();
    println!("You are currently running: {:?}", session);

    let is_sway = matches!(session, identifier::Desktop::Sway);
    let compositor = init_compositor(session);
    if let Some(ref comp) = compositor {
        // Restore any settings persisted to generated-config.d from a
        // previous cosmolith/Sway run, so a cosmolith restart doesn't lose
        // state that hasn't been re-emitted by cosmic-config yet.
        match comp.replay_persisted_config() {
            Ok(count) if count > 0 => {
                println!("Replayed {count} persisted setting(s) from generated-config.d");
            }
            Ok(_) => {}
            Err(err) => eprintln!("Failed to replay persisted generated-config.d settings: {err}"),
        }
    } else {
        eprintln!("No supported compositor detected. Events will be logged only.");
    }

    let _display_watcher = if is_sway {
        Some(display_persistence::start_output_watcher(
            display_persistence::config_path(),
        ))
    } else {
        None
    };

    loop {
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(event) => {
                println!("Recieved: {:?}", event);
                if let Some(ref comp) = compositor {
                    match comp.apply_event(event) {
                        Ok(()) => {
                            // println!("successfull.");
                        }
                        Err(err) => {
                            eprintln!("Failed to apply event: {err}");
                        }
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // optional heartbeat to keep the loop responsive to Ctrl+C
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                eprintln!("Watcher channel closed; exiting.");
                break;
            }
        }
    }

    Ok(())
}
