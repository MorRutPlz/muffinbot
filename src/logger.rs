#[macro_export]
macro_rules! debug {
    ($($arg:tt)+) => (
        log::log!(target: "muffinbot", log::Level::Debug, $($arg)+)
    )
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)+) => (
        log::log!(target: "muffinbot", log::Level::Error, $($arg)+)
    )
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)+) => (
        log::log!(target: "muffinbot", log::Level::Warn, $($arg)+)
    )
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)+) => (
        log::log!(target: "muffinbot", log::Level::Info, $($arg)+)
    )
}
