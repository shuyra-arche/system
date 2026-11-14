use ::resource::dock::View;

pub struct GuardSettings {
    pub target: &'static str,
    pub drain: fn(View, &mut [u8]) -> usize,
}
