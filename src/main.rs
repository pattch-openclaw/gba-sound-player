#![no_std]
#![no_main]

// use agb::display::{tiled::Tiled0, Priority};

#[agb::entry]
fn main(mut _gba: agb::Gba) -> ! {
    loop {
        agb::display::busy_wait_for_vblank();
    }
}
