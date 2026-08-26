#![no_std]
#![no_main]
#![cfg_attr(test, feature(custom_test_frameworks))]
#![cfg_attr(test, test_runner(agb::test_runner::test_runner))]
#![cfg_attr(test, reexport_test_harness_main = "test_main")]

pub mod pcm;

#[agb::entry]
fn main(mut _gba: agb::Gba) -> ! {
    #[cfg(test)]
    test_main();

    loop {
        agb::display::busy_wait_for_vblank();
    }
}
