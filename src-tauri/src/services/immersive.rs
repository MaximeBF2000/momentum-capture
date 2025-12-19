#[derive(Debug, Default)]
pub struct ImmersiveMode {
    enabled: bool,
}

impl ImmersiveMode {
    pub fn new() -> Self {
        Self { enabled: false }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}
