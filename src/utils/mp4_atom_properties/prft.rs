use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Prft,
};

impl AtomWithProperties for Prft {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "ProducerReferenceTimeBox",
            vec![
                (
                    "reference_track_id",
                    AtomPropertyValue::from(self.reference_track_id),
                ),
                ("ntp_timestamp", AtomPropertyValue::from(self.ntp_timestamp)),
                ("media_time", AtomPropertyValue::from(self.media_time)),
                (
                    "ntp_timestamp_media_time_association",
                    AtomPropertyValue::from(format!(
                        "{}",
                        self.ntp_timestamp_media_time_association
                    )),
                ),
            ],
        )
    }
}
