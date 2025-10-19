use crate::utils::mp4_atom_properties::{
    AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue, TablePropertyValue,
};
use mp4_atom::Tkhd;

impl AtomWithProperties for Tkhd {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "TrackHeaderBox",
            properties: vec![
                ("creation_time", AtomPropertyValue::from(self.creation_time)),
                (
                    "modification_time",
                    AtomPropertyValue::from(self.modification_time),
                ),
                ("track_id", AtomPropertyValue::from(self.track_id)),
                ("duration", AtomPropertyValue::from(self.duration)),
                ("layer", AtomPropertyValue::from(self.layer)),
                (
                    "alternate_group",
                    AtomPropertyValue::from(self.alternate_group),
                ),
                ("enabled", AtomPropertyValue::from(self.enabled)),
                (
                    "volume",
                    AtomPropertyValue::from(format!("{:?}", self.volume)),
                ),
                (
                    "matrix",
                    AtomPropertyValue::Table(TablePropertyValue {
                        headers: None,
                        rows: vec![
                            vec![
                                BasicPropertyValue::from(self.matrix.a),
                                BasicPropertyValue::from(self.matrix.b),
                                BasicPropertyValue::from(self.matrix.u),
                            ],
                            vec![
                                BasicPropertyValue::from(self.matrix.c),
                                BasicPropertyValue::from(self.matrix.d),
                                BasicPropertyValue::from(self.matrix.v),
                            ],
                            vec![
                                BasicPropertyValue::from(self.matrix.x),
                                BasicPropertyValue::from(self.matrix.y),
                                BasicPropertyValue::from(self.matrix.w),
                            ],
                        ],
                    }),
                ),
                (
                    "width",
                    AtomPropertyValue::from(format!("{:?}", self.width)),
                ),
                (
                    "height",
                    AtomPropertyValue::from(format!("{:?}", self.height)),
                ),
            ],
        }
    }
}
