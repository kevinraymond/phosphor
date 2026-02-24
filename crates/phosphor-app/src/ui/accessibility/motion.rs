/// Detects and tracks prefers-reduced-motion state.
pub struct ReducedMotion {
    pub enabled: bool,
    pub user_override: Option<bool>,
}

impl ReducedMotion {
    pub fn detect() -> Self {
        let system = detect_system_preference();
        Self {
            enabled: system,
            user_override: None,
        }
    }

    pub fn should_reduce(&self) -> bool {
        self.user_override.unwrap_or(self.enabled)
    }

    pub fn set_override(&mut self, value: Option<bool>) {
        self.user_override = value;
    }
}

#[cfg(target_os = "linux")]
fn detect_system_preference() -> bool {
    // Try reading from D-Bus org.gnome.desktop.interface enable-animations
    // For now, default to false (don't reduce)
    std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "enable-animations"])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim() == "false")
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn detect_system_preference() -> bool {
    // NSWorkspace.accessibilityDisplayShouldReduceMotion
    false // TODO: implement via objc
}

#[cfg(target_os = "windows")]
fn detect_system_preference() -> bool {
    // SPI_GETCLIENTAREAANIMATION
    false // TODO: implement via winapi
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn detect_system_preference() -> bool {
    false
}
