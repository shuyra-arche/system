pub(crate) fn emit(event: unit::event::Event<&str>) -> Result<(), &'static str> {
    let frame = unit::event::encode(&event).map_err(|_| "event encoding")?;
    let bytes = frame.as_bytes();
    let written = env::debug::put(env::VirtAddr::new(bytes.as_ptr() as usize), bytes.len())
        .map_err(|_| "event delivery")?;
    if written != bytes.len() {
        return Err("partial event delivery");
    }
    Ok(())
}
