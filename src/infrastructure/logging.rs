// Bring two helper traits from `tracing_subscriber` into scope.
//
// In Rust, methods can come from traits.
// The `.with(...)` and `.init()` methods used below are provided by these traits,
// so we import them before calling those methods.
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// This function sets up logging/tracing for the whole backend application.
//
// `pub` means other modules can call this function.
// `fn` means we are defining a function.
// `init_tracing` is the function name.
// `()` means this function takes no arguments.
pub fn init_tracing() {
    // Start building a tracing subscriber registry.
    //
    // A "subscriber" is the part of the tracing system that receives logs/events.
    // Without a subscriber, calls like `tracing::info!(...)` would not know
    // where or how to print logs.
    tracing_subscriber::registry()
        // Add a filtering layer.
        //
        // A filter decides which logs should be shown and which should be hidden.
        // For example, you may want debug logs during development,
        // but only info/error logs in production.
        .with(
            // Try to read the log filter from the environment.
            //
            // By default, this checks the `RUST_LOG` environment variable.
            //
            // Example:
            // RUST_LOG=debug
            //
            // Another example:
            // RUST_LOG=korede_backend=debug,tower_http=info
            tracing_subscriber::EnvFilter::try_from_default_env()
                // If `RUST_LOG` is missing or invalid, use this default filter.
                //
                // `|_|` means:
                // "I receive the error, but I do not need to use it."
                //
                // `"korede_backend=debug,tower_http=debug"` means:
                // - show debug logs from your backend crate
                // - show debug logs from `tower_http`, which logs HTTP requests
                //
                // `.into()` converts the string into the type `EnvFilter` expects.
                .unwrap_or_else(|_| "korede_backend=debug,tower_http=debug".into()),
        )
        // Add a formatting layer.
        //
        // This controls how logs are displayed in the terminal.
        // The default formatter prints useful details like:
        // - timestamp
        // - log level
        // - module path
        // - message
        .with(tracing_subscriber::fmt::layer())
        // Activate this subscriber globally.
        //
        // This should usually be called once when the app starts.
        // After this, any `tracing::info!`, `tracing::error!`, etc.
        // will go through the logging setup above.
        .init();
}
