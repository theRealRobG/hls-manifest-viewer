use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Ccst;

impl AtomWithProperties for Ccst {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "CodingConstraintsBox",
            properties: vec![
                (
                    "all_ref_pics_intra",
                    AtomPropertyValue::from(self.all_ref_pics_intra),
                ),
                (
                    "intra_pred_used",
                    AtomPropertyValue::from(self.intra_pred_used),
                ),
                (
                    "max_ref_per_pic",
                    AtomPropertyValue::from(self.max_ref_per_pic),
                ),
            ],
        }
    }
}
