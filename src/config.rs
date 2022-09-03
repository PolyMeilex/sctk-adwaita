//! System configuration.
use crate::title::font_preference::FontPreference;
use std::process::Command;

/// Query system to see if dark theming should be preferred.
pub(crate) fn prefer_dark() -> bool {
    // outputs something like: `variant       variant          uint32 1`
    let stdout = Command::new("dbus-send")
        .arg("--print-reply=literal")
        .arg("--dest=org.freedesktop.portal.Desktop")
        .arg("/org/freedesktop/portal/desktop")
        .arg("org.freedesktop.portal.Settings.Read")
        .arg("string:org.freedesktop.appearance")
        .arg("string:color-scheme")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok());

    matches!(stdout, Some(s) if s.trim().ends_with("uint32 1"))
}

/// Query system for which font to use for window titles.
pub(crate) fn titlebar_font() -> Option<FontPreference> {
    // outputs something like: `'Cantarell Bold 12'`
    let stdout = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.wm.preferences", "titlebar-font"])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())?;

    FontPreference::from_name_style_size(
        stdout
            .trim()
            .trim_end_matches('\'')
            .trim_start_matches('\''),
    )
}
