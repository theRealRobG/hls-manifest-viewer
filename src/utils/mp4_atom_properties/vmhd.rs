use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Vmhd;

impl AtomWithProperties for Vmhd {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "VideoMediaHeaderBox",
            vec![
                ("graphics_mode", AtomPropertyValue::from(self.graphics_mode)),
                (
                    "op_color",
                    AtomPropertyValue::from(format!(
                        "r:{}, g:{}, b:{}",
                        self.op_color.red, self.op_color.green, self.op_color.blue
                    )),
                ),
            ],
        )
    }
}
