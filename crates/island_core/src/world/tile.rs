//! What a single tile is.

/// The material of a tile's top surface.
///
/// `#[repr(u8)]` because tiles are stored one byte each and will eventually be
/// serialised. The discriminants are written out explicitly and **must not be
/// reordered** once worlds are saved to disk — a save file stores the number,
/// not the name.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TileKind {
    Water = 0,
    Sand = 1,
    Grass = 2,
    Rock = 3,
    Snow = 4,
}

impl TileKind {
    /// Every kind, in discriminant order. Kept next to the enum so a new
    /// variant that is not added here is caught by the exhaustiveness test.
    pub const ALL: [TileKind; 5] = [
        TileKind::Water,
        TileKind::Sand,
        TileKind::Grass,
        TileKind::Rock,
        TileKind::Snow,
    ];

    /// Linear RGB for the tile's top surface.
    ///
    /// Deliberately an exhaustive `match` with no wildcard arm: adding a
    /// `TileKind` should fail to compile until someone chooses its colour,
    /// rather than silently rendering as whatever the fallback was.
    ///
    /// Vertex colours are a stand-in until there is a texture pipeline. The
    /// values are picked to keep the elevation bands distinguishable from each
    /// other under the flat directional light, not to look good.
    pub const fn color(self) -> [f32; 3] {
        match self {
            TileKind::Water => [0.09, 0.24, 0.44],
            TileKind::Sand => [0.76, 0.68, 0.45],
            TileKind::Grass => [0.24, 0.44, 0.20],
            TileKind::Rock => [0.40, 0.39, 0.38],
            TileKind::Snow => [0.86, 0.88, 0.92],
        }
    }

    /// Short lowercase name, for logs and the debug readout.
    pub const fn name(self) -> &'static str {
        match self {
            TileKind::Water => "water",
            TileKind::Sand => "sand",
            TileKind::Grass => "grass",
            TileKind::Rock => "rock",
            TileKind::Snow => "snow",
        }
    }

    /// Reconstruct from a stored discriminant, e.g. when loading a save.
    /// Returns `None` for an unknown value rather than transmuting, which
    /// would be instant undefined behaviour on a corrupt or newer file.
    pub const fn from_u8(value: u8) -> Option<TileKind> {
        match value {
            0 => Some(TileKind::Water),
            1 => Some(TileKind::Sand),
            2 => Some(TileKind::Grass),
            3 => Some(TileKind::Rock),
            4 => Some(TileKind::Snow),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_contains_every_kind_in_discriminant_order() {
        // If a variant is added without updating ALL, this catches it: the
        // round-trip below would skip the new discriminant.
        for (i, kind) in TileKind::ALL.iter().enumerate() {
            assert_eq!(
                *kind as u8, i as u8,
                "TileKind::ALL must be in discriminant order"
            );
        }
        assert_eq!(
            TileKind::from_u8(TileKind::ALL.len() as u8),
            None,
            "ALL is missing a variant, or from_u8 has one ALL does not"
        );
    }

    #[test]
    fn from_u8_round_trips_every_kind() {
        for kind in TileKind::ALL {
            assert_eq!(TileKind::from_u8(kind as u8), Some(kind));
        }
    }

    #[test]
    fn from_u8_rejects_unknown_discriminants() {
        assert_eq!(TileKind::from_u8(200), None);
        assert_eq!(TileKind::from_u8(u8::MAX), None);
    }

    #[test]
    fn colors_are_in_unit_range_and_distinct() {
        for kind in TileKind::ALL {
            for c in kind.color() {
                assert!(
                    (0.0..=1.0).contains(&c),
                    "{} has out-of-range colour",
                    kind.name()
                );
            }
        }
        // Two kinds sharing a colour would make them indistinguishable on
        // screen, which reads as a generation bug rather than a palette one.
        for (i, a) in TileKind::ALL.iter().enumerate() {
            for b in &TileKind::ALL[i + 1..] {
                assert_ne!(
                    a.color(),
                    b.color(),
                    "{} and {} share a colour",
                    a.name(),
                    b.name()
                );
            }
        }
    }
}
