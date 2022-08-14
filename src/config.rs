//! System configuration.
use std::process::Command;

/// Query system to see if dark theming should be preferred.
pub(crate) fn prefer_dark() -> bool {
    let gsettings_color_scheme = Command::new("gsettings")
        .args(&["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok());
    let color_scheme = gsettings_color_scheme
        .as_ref()
        .map(|o| o.trim().trim_matches('\''));

    matches!(color_scheme, Some("prefer-dark"))
}
