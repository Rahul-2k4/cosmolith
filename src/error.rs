use std::error::Error as StdError;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("failed to initialise config for {namespace}")]
    ConfigInit {
        namespace: &'static str,
        #[source]
        source: Box<dyn StdError + 'static>,
    },
    #[error("failed to read config key {namespace}.{key}")]
    ConfigRead {
        namespace: &'static str,
        key: String,
        #[source]
        source: Box<dyn StdError + 'static>,
    },
    #[error("failed to set up {watcher} watcher")]
    WatcherSetup {
        watcher: &'static str,
        #[source]
        source: Box<dyn StdError + 'static>,
    },
    #[error("failed to lock the {channel} event channel")]
    ChannelLock { channel: &'static str },
    #[error("failed to send {channel} event")]
    ChannelSend {
        channel: &'static str,
        #[source]
        source: Box<dyn StdError + 'static>,
    },
}

impl Error {
    pub fn config_init<E>(namespace: &'static str, source: E) -> Self
    where
        E: StdError + 'static,
    {
        Self::ConfigInit {
            namespace,
            source: Box::new(source),
        }
    }

    pub fn config_read<E>(namespace: &'static str, key: impl Into<String>, source: E) -> Self
    where
        E: StdError + 'static,
    {
        Self::ConfigRead {
            namespace,
            key: key.into(),
            source: Box::new(source),
        }
    }

    pub fn watcher_setup<E>(watcher: &'static str, source: E) -> Self
    where
        E: StdError + 'static,
    {
        Self::WatcherSetup {
            watcher,
            source: Box::new(source),
        }
    }

    pub fn channel_lock(channel: &'static str) -> Self {
        Self::ChannelLock { channel }
    }

    pub fn channel_send<E>(channel: &'static str, source: E) -> Self
    where
        E: StdError + 'static,
    {
        Self::ChannelSend {
            channel,
            source: Box::new(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Error;
    use std::{error::Error as StdError, io};

    #[test]
    fn config_read_has_context_and_source() {
        let error = Error::config_read(
            "com.system76.CosmicComp",
            "xkb_config",
            io::Error::new(io::ErrorKind::NotFound, "missing key"),
        );

        assert_eq!(
            error.to_string(),
            "failed to read config key com.system76.CosmicComp.xkb_config"
        );
        assert_eq!(error.source().unwrap().to_string(), "missing key");
    }

    #[test]
    fn channel_send_has_context_and_source() {
        let error = Error::channel_send(
            "input",
            io::Error::new(io::ErrorKind::BrokenPipe, "receiver closed"),
        );

        assert_eq!(error.to_string(), "failed to send input event");
        assert_eq!(error.source().unwrap().to_string(), "receiver closed");
    }
}
