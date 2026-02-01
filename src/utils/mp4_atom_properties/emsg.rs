use crate::utils::mp4_atom_properties::{
    AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue, TablePropertyValue,
};
use mp4_atom::Emsg;

impl AtomWithProperties for Emsg {
    fn properties(&self) -> AtomProperties {
        let message_data = if &self.scheme_id_uri == "https://aomedia.org/emsg/ID3" {
            let message_data_reader = std::io::Cursor::new(self.message_data.clone());
            match id3::Tag::read_from2(message_data_reader) {
                Ok(id3_tag) => {
                    let mut tags = Vec::new();
                    for frame in id3_tag.frames() {
                        let id = frame.id();
                        let value = format!("{}", frame.content());
                        tags.push((id, value));
                    }
                    AtomPropertyValue::Table(TablePropertyValue {
                        headers: Some(vec!["id3 frame ID", "Value"]),
                        rows: tags
                            .iter()
                            .map(|(name, value)| {
                                vec![
                                    BasicPropertyValue::from(*name),
                                    BasicPropertyValue::from(value),
                                ]
                            })
                            .collect(),
                    })
                }
                Err(_) => {
                    AtomPropertyValue::from(String::from_utf8_lossy(&self.message_data).to_string())
                }
            }
        } else {
            AtomPropertyValue::from(String::from_utf8_lossy(&self.message_data).to_string())
        };
        AtomProperties::from_static_keys(
            "EventMessageBox",
            vec![
                ("timescale", AtomPropertyValue::from(self.timescale)),
                match self.presentation_time {
                    mp4_atom::EmsgTimestamp::Relative(t) => {
                        ("presentation_time_delta", AtomPropertyValue::from(t))
                    }
                    mp4_atom::EmsgTimestamp::Absolute(t) => {
                        ("presentation_time", AtomPropertyValue::from(t))
                    }
                },
                (
                    "event_duration",
                    AtomPropertyValue::from(self.event_duration),
                ),
                ("id", AtomPropertyValue::from(self.id)),
                (
                    "scheme_id_uri",
                    AtomPropertyValue::from(&self.scheme_id_uri),
                ),
                ("value", AtomPropertyValue::from(&self.value)),
                ("message_data", message_data),
            ],
        )
    }
}
