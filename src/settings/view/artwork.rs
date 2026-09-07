//! Immutable Chronicle illustration tiles, loaded once from embedded resources.

use std::sync::OnceLock;

use crate::theme::ThemeMode;

type Tile = (u16, u16, u16, u16, u32);

pub(super) fn chronicle_art_tiles() -> &'static [Tile] {
    static TILES: OnceLock<Vec<Tile>> = OnceLock::new();
    TILES.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../assets/illustrations/chronicle-art-tiles.json"
        ))
        .expect("valid embedded Chronicle artwork")
    })
}

pub(super) fn chronicle_inner_tiles(mode: ThemeMode) -> &'static [Tile] {
    static LIGHT: OnceLock<Vec<Tile>> = OnceLock::new();
    static DARK: OnceLock<Vec<Tile>> = OnceLock::new();
    let (cache, source) = match mode {
        ThemeMode::Light => (
            &LIGHT,
            include_str!("../../../assets/illustrations/chronicle-inner-light-tiles.json"),
        ),
        ThemeMode::Dark => (
            &DARK,
            include_str!("../../../assets/illustrations/chronicle-inner-dark-tiles.json"),
        ),
    };
    cache.get_or_init(|| {
        serde_json::from_str(source).expect("valid embedded Chronicle inner artwork")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_artwork_fits_its_reference_canvas() {
        for (tiles, canvas) in [
            (chronicle_art_tiles(), (384, 342)),
            (chronicle_inner_tiles(ThemeMode::Light), (342, 300)),
            (chronicle_inner_tiles(ThemeMode::Dark), (342, 300)),
        ] {
            assert!(!tiles.is_empty());
            for &(x, y, width, height, _) in tiles {
                assert!(width > 0 && height > 0);
                assert!(u32::from(x) + u32::from(width) <= canvas.0);
                assert!(u32::from(y) + u32::from(height) <= canvas.1);
            }
        }
    }
}
