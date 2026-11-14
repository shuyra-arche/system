use core::sync::atomic::{AtomicBool, AtomicUsize};

pub struct Signals {
    pub pings: AtomicUsize,
    pub stage: AtomicUsize,
    pub ack: AtomicUsize,
    pub target: AtomicUsize,
    pub peer_done: AtomicBool,
    pub done: AtomicBool,
    pub entry: AtomicUsize,
    pub terminal_entry: AtomicUsize,
    pub terminal_target: AtomicUsize,
    pub terminal_ready: AtomicUsize,
    pub inherited_ready: AtomicBool,
    pub inherited_done: AtomicBool,
    pub creator_request: AtomicBool,
    pub creator_ready: AtomicBool,
}
