use crate::utils::{
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Dvvc,
};

impl AtomWithProperties for Dvvc {
    fn properties(&self) -> AtomProperties {
        AtomProperties::from_static_keys(
            "DOVIDecoderConfigurationRecord",
            vec![
                (
                    "dv_version_major",
                    AtomPropertyValue::from(self.dv_version_major),
                ),
                (
                    "dv_version_minor",
                    AtomPropertyValue::from(self.dv_version_minor),
                ),
                ("dv_profile", AtomPropertyValue::from(self.dv_profile)),
                ("dv_level", AtomPropertyValue::from(self.dv_level)),
                (
                    "rpu_present_flag",
                    AtomPropertyValue::from(self.rpu_present),
                ),
                ("el_present_flag", AtomPropertyValue::from(self.el_present)),
                ("bl_present_flag", AtomPropertyValue::from(self.bl_present)),
                (
                    "dv_bl_signal_compatibility_id",
                    AtomPropertyValue::from(self.dv_bl_signal_compatibility_id),
                ),
                (
                    "dv_md_compression",
                    AtomPropertyValue::from(self.dv_md_compression),
                ),
            ],
        )
    }
}
