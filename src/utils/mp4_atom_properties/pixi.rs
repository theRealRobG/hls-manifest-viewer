use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Pixi;

impl AtomWithProperties for Pixi {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "PixelInformationProperty",
            properties: vec![(
                "bits_per_channel",
                AtomPropertyValue::from(
                    self.bits_per_channel
                        .iter()
                        .map(|bits| format!("{bits}"))
                        .collect::<Vec<String>>()
                        .join(", "),
                ),
            )],
        }
    }
}
