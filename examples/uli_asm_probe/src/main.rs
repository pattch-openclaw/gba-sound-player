//! Minimal on-target probe for the hand-written ARM FFT (PR C-1).
//!
//! Runs `uli_lite_gba::fft` (the Rust ARM-mode kernel) and
//! `uli_lite_gba::fft_asm_call` (the asm FFT) on identical `pre` output and
//! reports whether they match, plus the first divergent index and both values.
//! It is a debugging image, not a gate: it exists so the asm can be iterated on
//! in seconds instead of through the full bench.

#![no_std]
#![no_main]

use agb::display::Rgb;
use uli_lite::format::{H, M};
use uli_lite_gba as overlay;

const COLOR_PASS: agb::display::Rgb15 = Rgb::new(0, 0, 255).to_rgb15();
const COLOR_FAIL: agb::display::Rgb15 = Rgb::new(255, 0, 0).to_rgb15();

fn fill_kx(buf: &mut [i32; M]) {
    let mut s: u32 = 0x1234_5678;
    for v in buf.iter_mut() {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        *v = (s as i32) >> 4;
    }
}

fn run() -> bool {
    let mut x = [0i32; M];
    fill_kx(&mut x);

    let mut z_ref = [0i32; 2 * H];
    let mut z_asm = [0i32; 2 * H];
    overlay::pre(&x, &mut z_ref);
    overlay::pre(&x, &mut z_asm);

    let z_asm2 = z_asm;
    let z_asm3 = z_asm;
    let mut a = z_ref;
    let mut b = z_asm;
    let mut b2 = z_asm2;
    let mut b3 = z_asm3;
    agb::println!(
        "[asm-probe] z[0,1,128,129,256,257,384,385]={} {} {} {} {} {} {} {}",
        b[0],
        b[1],
        b[128],
        b[129],
        b[256],
        b[257],
        b[384],
        b[385]
    );
    agb::println!("[asm-probe] calling Rust ARM fft");
    overlay::fft(&mut a);
    agb::println!(
        "[asm-probe] rust a[0..8]={} {} {} {} {} {} {} {}",
        a[0],
        a[1],
        a[2],
        a[3],
        a[4],
        a[5],
        a[6],
        a[7]
    );
    agb::println!("[asm-probe] calling asm fft v1");
    overlay::fft_asm_call(&mut b);
    agb::println!("[asm-probe] calling asm fft v2");
    overlay::fft_asm_v2_call(&mut b2);
    agb::println!("[asm-probe] calling asm fft v3");
    overlay::fft_asm_v3_call(&mut b3);
    agb::println!(
        "[asm-probe] asm b[0..8]={} {} {} {} {} {} {} {}",
        b[0],
        b[1],
        b[2],
        b[3],
        b[4],
        b[5],
        b[6],
        b[7]
    );

    let mut bad1: i32 = -1;
    for i in 0..2 * H {
        if a[i] != b[i] {
            bad1 = i as i32;
            break;
        }
    }
    let mut bad2: i32 = -1;
    for i in 0..2 * H {
        if a[i] != b2[i] {
            bad2 = i as i32;
            break;
        }
    }
    if bad1 >= 0 {
        let i = bad1 as usize;
        agb::println!(
            "[asm-probe] v1 MISMATCH idx={} rust={} v1={}",
            i,
            a[i],
            b[i]
        );
    } else {
        agb::println!("[asm-probe] v1 matches");
    }
    if bad2 >= 0 {
        let i = bad2 as usize;
        agb::println!(
            "[asm-probe] v2 MISMATCH idx={} rust={} v2={}",
            i,
            a[i],
            b2[i]
        );
    } else {
        agb::println!("[asm-probe] v2 matches");
    }
    let mut bad3: i32 = -1;
    for i in 0..2 * H {
        if a[i] != b3[i] {
            bad3 = i as i32;
            break;
        }
    }
    if bad3 >= 0 {
        let i = bad3 as usize;
        agb::println!(
            "[asm-probe] v3 MISMATCH idx={} rust={} v3={}",
            i,
            a[i],
            b3[i]
        );
    } else {
        agb::println!("[asm-probe] v3 matches");
    }
    bad1 < 0 && bad2 < 0 && bad3 < 0
}

#[agb::entry]
fn main(mut gba: agb::Gba) -> ! {
    let pass = run();
    agb::println!("[asm-probe] {}", if pass { "PASS" } else { "FAIL" });
    agb::println!("Tests finished successfully");
    let mut gfx = gba.graphics.get();
    gfx.set_background_palette_colour(0, 0, if pass { COLOR_PASS } else { COLOR_FAIL });
    loop {
        let frame = gfx.frame();
        frame.commit();
    }
}
