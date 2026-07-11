use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("failed to initialize {component} watcher: {source}")]
    WatcherInit {
        component: &'static str,
        #[source]
        source: Box<dyn std::error::Error + 'static>,
    },
    #[error("failed to send {component} event: {source}")]
    EventSend {
        component: &'static str,
        #[source]
        source: std::sync::mpsc::SendError<crate::event::Event>,
    },
}

impl Error {
    pub fn watcher_init<E>(component: &'static str, source: E) -> Self
    where
        E: std::error::Error + 'static,
    {
        Self::WatcherInit {
            component,
            source: Box::new(source),
        }
    }

    pub fn event_send(
        component: &'static str,
        source: std::sync::mpsc::SendError<crate::event::Event>,
    ) -> Self {
        Self::EventSend { component, source }
    }
}

#[cfg(test)]
mod tests {
    use super::Error;
    use std::error::Error as _;

    #[test]
    fn watcher_error_preserves_source_and_context() {
        let error = Error::watcher_init(
            "input",
            std::io::Error::new(std::io::ErrorKind::NotFound, "configuration unavailable"),
        );

        assert_eq!(
            error.to_string(),
            "failed to initialize input watcher: configuration unavailable"
        );
        let source = error.source().expect("source error");
        assert_eq!(source.to_string(), "configuration unavailable");
        assert!(source.downcast_ref::<std::io::Error>().is_some());
    }
}
