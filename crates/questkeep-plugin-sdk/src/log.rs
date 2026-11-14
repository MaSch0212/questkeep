//! Lines in Questkeep's log (tagged with the plugin's id). Outside the app (native builds, unit
//! tests) they are dropped.

#[cfg(target_arch = "wasm32")]
fn write(level: crate::bindings::questkeep::plugin::host::LogLevel, message: &str) {
    crate::bindings::questkeep::plugin::host::log(level, message);
}

macro_rules! level {
    ($name:ident, $variant:ident) => {
        pub fn $name(message: impl AsRef<str>) {
            #[cfg(target_arch = "wasm32")]
            write(
                crate::bindings::questkeep::plugin::host::LogLevel::$variant,
                message.as_ref(),
            );
            #[cfg(not(target_arch = "wasm32"))]
            let _ = message;
        }
    };
}

level!(debug, Debug);
level!(info, Info);
level!(warn, Warn);
level!(error, Error);
