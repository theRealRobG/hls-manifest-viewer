use crate::utils::mp4_atom_properties::{
    AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue, TablePropertyValue,
};
use mp4_atom::Mvhd;

impl AtomWithProperties for Mvhd {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "MovieHeaderBox",
            vec![
                ("creation_time", AtomPropertyValue::from(self.creation_time)),
                (
                    "modification_time",
                    AtomPropertyValue::from(self.modification_time),
                ),
                ("timescale", AtomPropertyValue::from(self.timescale)),
                ("duration", AtomPropertyValue::from(self.duration)),
                ("rate", AtomPropertyValue::from(format!("{:?}", self.rate))),
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
                ("next_track_id", AtomPropertyValue::from(self.next_track_id)),
            ],
        )
    }
}
