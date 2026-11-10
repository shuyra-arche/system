#![no_std]
#![no_main]

#[mold::entry]
fn main() -> Result<(), env::Reason> {
    let boot = system_server::boot::take().map_err(|error| error.code())?;
    system_server::run(boot)
}
