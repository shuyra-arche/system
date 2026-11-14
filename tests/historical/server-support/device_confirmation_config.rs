pub struct ConfirmationConfig {
    pub party: u8,
    pub op: u8,
    pub target: &'static str,
    pub primitive: bool,
    pub before_stop: bool,
}
