use crate::utils::mp4_atom_properties::{
    array_string_from, AtomProperties, AtomPropertyValue, AtomWithProperties,
};
use mp4_atom::Tx3g;

impl AtomWithProperties for Tx3g {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "3GPP Timed Text",
            properties: vec![
                (
                    "data_reference_index",
                    AtomPropertyValue::from(self.data_reference_index),
                ),
                ("display_flags", AtomPropertyValue::from(self.display_flags)),
                (
                    "horizontal_justification",
                    AtomPropertyValue::from(self.horizontal_justification),
                ),
                (
                    "vertical_justification",
                    AtomPropertyValue::from(self.vertical_justification),
                ),
                (
                    "bg_color_rgba",
                    AtomPropertyValue::from(format!(
                        "r:{},g:{},b:{},a:{}",
                        self.bg_color_rgba.red,
                        self.bg_color_rgba.green,
                        self.bg_color_rgba.blue,
                        self.bg_color_rgba.alpha
                    )),
                ),
                (
                    "box_record",
                    AtomPropertyValue::from(format!(
                        "{}, {}, {}, {}",
                        self.box_record[0],
                        self.box_record[1],
                        self.box_record[2],
                        self.box_record[3]
                    )),
                ),
                (
                    "style_record",
                    AtomPropertyValue::from(array_string_from(&self.style_record)),
                ),
            ],
        }
    }
}
