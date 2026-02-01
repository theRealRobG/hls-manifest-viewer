use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Clap;

impl AtomWithProperties for Clap {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "CleanApertureBox",
            vec![
                (
                    "clean_aperture_width_n",
                    AtomPropertyValue::from(self.clean_aperture_width_n),
                ),
                (
                    "clean_aperture_width_d",
                    AtomPropertyValue::from(self.clean_aperture_width_d),
                ),
                (
                    "clean_aperture_height_n",
                    AtomPropertyValue::from(self.clean_aperture_height_n),
                ),
                (
                    "clean_aperture_height_d",
                    AtomPropertyValue::from(self.clean_aperture_height_d),
                ),
                ("horiz_off_n", AtomPropertyValue::from(self.horiz_off_n)),
                ("horiz_off_d", AtomPropertyValue::from(self.horiz_off_d)),
                ("vert_off_n", AtomPropertyValue::from(self.vert_off_n)),
                ("vert_off_d", AtomPropertyValue::from(self.vert_off_d)),
            ],
        )
    }
}
