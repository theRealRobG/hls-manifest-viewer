use crate::utils::mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties};
use mp4_atom::Esds;

impl AtomWithProperties for Esds {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "ElementaryStreamDescriptorBox",
            vec![
                ("es_id", AtomPropertyValue::from(self.es_desc.es_id)),
                (
                    "decoder_config_object_type_indication",
                    AtomPropertyValue::from(self.es_desc.dec_config.object_type_indication),
                ),
                (
                    "decoder_config_stream_type",
                    AtomPropertyValue::from(self.es_desc.dec_config.stream_type),
                ),
                (
                    "decoder_config_up_stream",
                    AtomPropertyValue::from(self.es_desc.dec_config.up_stream),
                ),
                (
                    "decoder_config_buffer_size_db",
                    AtomPropertyValue::from(u32::from(self.es_desc.dec_config.buffer_size_db)),
                ),
                (
                    "decoder_config_max_bitrate",
                    AtomPropertyValue::from(self.es_desc.dec_config.max_bitrate),
                ),
                (
                    "decoder_config_avg_bitrate",
                    AtomPropertyValue::from(self.es_desc.dec_config.avg_bitrate),
                ),
                (
                    "decoder_specific_profile",
                    AtomPropertyValue::from(self.es_desc.dec_config.dec_specific.profile),
                ),
                (
                    "decoder_specific_freq_index",
                    AtomPropertyValue::from(self.es_desc.dec_config.dec_specific.freq_index),
                ),
                (
                    "decoder_specific_chan_conf",
                    AtomPropertyValue::from(self.es_desc.dec_config.dec_specific.chan_conf),
                ),
            ],
        )
    }
}
