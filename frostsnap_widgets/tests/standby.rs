use embedded_graphics::{geometry::Size, pixelcolor::Rgb565, prelude::Point};
use frostsnap_core::{
    device::KeyPurpose, message::HeldShare2, schnorr_fun::frost::SecretShare, AccessStructureId,
    AccessStructureRef, KeyId,
};
use frostsnap_widgets::{
    palette::PALETTE, vec_framebuffer::VecFramebuffer, DynWidget, Instant, Standby,
    SuperDrawTarget, Widget,
};

const SCREEN: Size = Size::new(240, 280);

fn held_share(recovery_mode: bool) -> HeldShare2 {
    use frostsnap_core::schnorr_fun::fun::prelude::*;
    HeldShare2 {
        access_structure_ref: (!recovery_mode).then_some(AccessStructureRef {
            key_id: KeyId([0u8; 32]),
            access_structure_id: AccessStructureId([1u8; 32]),
        }),
        share_image: SecretShare {
            index: Scalar::<Public, NonZero>::one(),
            share: Scalar::<Secret, Zero>::zero(),
        }
        .share_image(),
        threshold: Some(2),
        key_name: Some("Family Wallet".to_string()),
        purpose: Some(KeyPurpose::Bitcoin(bitcoin::Network::Bitcoin)),
        needs_consolidation: false,
    }
}

/// Draws past the startup delay and content fade so the screen has settled.
fn render(mut standby: Standby) -> VecFramebuffer<Rgb565> {
    standby.set_constraints(SCREEN);
    let mut target = SuperDrawTarget::new(
        VecFramebuffer::<Rgb565>::new(SCREEN.width as usize, SCREEN.height as usize),
        PALETTE.background,
    );
    for t in (0..=2000).step_by(50) {
        standby.draw(&mut target, Instant::from_millis(t)).unwrap();
    }
    target.into_inner().unwrap()
}

fn painted(fb: &VecFramebuffer<Rgb565>, x: u32, y: u32) -> bool {
    fb.get_pixel(Point::new(x as i32, y as i32))
        .is_some_and(|color| color != PALETTE.background)
}

fn screens() -> impl Iterator<Item = (&'static str, Standby)> {
    [
        (
            "startup",
            Standby::new(frostsnap_widgets::demo_widget::firmware_version()),
        ),
        ("welcome", {
            let mut standby = Standby::new(frostsnap_widgets::demo_widget::firmware_version());
            standby.set_welcome();
            standby
        }),
        ("has_key", {
            let mut standby = Standby::new(frostsnap_widgets::demo_widget::firmware_version());
            standby.set_key("Alice", held_share(false));
            standby
        }),
        ("recovery", {
            let mut standby = Standby::new(frostsnap_widgets::demo_widget::firmware_version());
            standby.set_key("Alice", held_share(true));
            standby
        }),
    ]
    .into_iter()
}

#[test]
fn every_standby_screen_shows_the_firmware_version() {
    for (name, standby) in screens() {
        let fb = render(standby);
        let corner = (SCREEN.width / 2..SCREEN.width)
            .flat_map(|x| (0..30).map(move |y| (x, y)))
            .any(|(x, y)| painted(&fb, x, y));
        assert!(corner, "{name}: nothing drawn in the version corner");
    }
}

/// The version first went at the foot of each content column, where the recovery
/// screen's extra warning row pushed it off the bottom of the display.
#[test]
fn no_standby_screen_is_clipped_by_the_bottom_edge() {
    for (name, standby) in screens() {
        let fb = render(standby);
        let bottom_row = SCREEN.height - 1;
        let clipped = (0..SCREEN.width).any(|x| painted(&fb, x, bottom_row));
        assert!(!clipped, "{name}: content runs into the bottom edge");
    }
}
