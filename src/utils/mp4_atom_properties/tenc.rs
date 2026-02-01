use crate::utils::{
    hex::encode_hex,
    mp4_atom_properties::{AtomProperties, AtomPropertyValue, AtomWithProperties},
    mp4_parsing::Tenc,
};

impl AtomWithProperties for Tenc {
    fn properties(&self) -> AtomProperties {
        let mut properties = vec![
            (
                "default_isProtected",
                AtomPropertyValue::from(format!(
                    "{} ({})",
                    self.default_is_protected,
                    self.is_protected(),
                )),
            ),
            (
                "default_Per_Sample_IV_Size",
                AtomPropertyValue::from(format!(
                    "{} ({})",
                    self.default_per_sample_iv_size,
                    self.per_sample_iv_size(),
                )),
            ),
            (
                "default_KID",
                AtomPropertyValue::from(encode_hex(&self.default_key_id)),
            ),
        ];
        if let Some(ref default_constant_iv) = self.default_constant_iv {
            properties.push((
                "default_constant_IV_size",
                AtomPropertyValue::from(format!(
                    "{} ({})",
                    default_constant_iv.len(),
                    self.constant_iv_size()
                )),
            ));
            properties.push((
                "default_constant_IV",
                AtomPropertyValue::from(encode_hex(default_constant_iv)),
            ));
        }
        if let Some(default_crypt_byte_block) = self.default_crypt_byte_block {
            properties.push((
                "default_crypt_byte_block",
                AtomPropertyValue::from(default_crypt_byte_block),
            ));
        }
        if let Some(default_skip_byte_block) = self.default_skip_byte_block {
            properties.push((
                "default_skip_byte_block",
                AtomPropertyValue::from(default_skip_byte_block),
            ));
        }
        AtomProperties {
            box_name: "TrackEncryptionBox",
            properties,
        }
    }
}
