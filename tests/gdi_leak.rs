//! Regression: overlay teardown must release the GDI objects it allocates.
//!
//! Ignored by default because it needs an interactive desktop; run locally with
//! `cargo test --test gdi_leak -- --ignored`.

use mouseshow::{Config, Marks};

unsafe extern "system" {
    fn GetCurrentProcess() -> isize;
    fn GetGuiResources(process: isize, flags: u32) -> u32;
}

const GR_GDIOBJECTS: u32 = 0;

fn gdi_objects() -> u32 {
    unsafe { GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS) }
}

#[test]
#[ignore = "needs an interactive desktop"]
fn repeated_start_stop_does_not_leak_gdi() {
    let before = gdi_objects();
    for _ in 0..3 {
        let marks = Marks::start(Config::default()).expect("overlay starts");
        drop(marks);
    }
    let after = gdi_objects();
    assert!(
        after <= before + 1,
        "GDI objects grew {before} -> {after} across three overlay cycles"
    );
}
