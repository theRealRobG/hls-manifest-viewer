use crate::utils::{
    hex::encode_hex,
    mp4::{Pssh, PsshData},
    mp4_atom_properties::{
        AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue,
        TablePropertyValue,
    },
    pssh_data::playready::PlayReadyRecordType,
};
use widevine_proto::license_protocol::widevine_pssh_data::{Algorithm, Type};

impl AtomWithProperties for Pssh {
    fn properties(&self) -> AtomProperties {
        AtomProperties {
            box_name: "ProtectionSystemSpecificHeaderBox",
            properties: vec![
                (
                    "system_id",
                    AtomPropertyValue::from(encode_hex(&self.system_id)),
                ),
                (
                    "system_ref",
                    AtomPropertyValue::from(self.system_reference().as_ref()),
                ),
                (
                    "key_ids",
                    AtomPropertyValue::Table(TablePropertyValue {
                        headers: None,
                        rows: self
                            .key_ids
                            .iter()
                            .map(|kid| vec![BasicPropertyValue::from(encode_hex(kid))])
                            .collect(),
                    }),
                ),
                (
                    "pssh_data",
                    match self.data.as_ref() {
                        Some(PsshData::PlayReady(data)) => {
                            let mut rows = Vec::new();
                            let should_add_row_headers = data.record.len() > 1;
                            let mut count = 0;
                            for record in &data.record {
                                count += 1;
                                if should_add_row_headers {
                                    rows.push(vec![
                                        BasicPropertyValue::from(format!("Record {count}")),
                                        BasicPropertyValue::from(""),
                                    ]);
                                }
                                rows.push(vec![
                                    BasicPropertyValue::from("type"),
                                    match record.record_type {
                                        PlayReadyRecordType::RightsManagement => {
                                            BasicPropertyValue::from("RightsManagement")
                                        }
                                        PlayReadyRecordType::Reserved => {
                                            BasicPropertyValue::from("Reserved")
                                        }
                                        PlayReadyRecordType::EmbeddedLicenseStore => {
                                            BasicPropertyValue::from("EmbeddedLicenseStore")
                                        }
                                    },
                                ]);
                                let header = &record.record_value;
                                rows.extend([
                                    vec![
                                        BasicPropertyValue::from("xmlns"),
                                        BasicPropertyValue::from(&header.xmlns),
                                    ],
                                    vec![
                                        BasicPropertyValue::from("version"),
                                        BasicPropertyValue::from(&header.version),
                                    ],
                                ]);
                                let mut kid_count = 0;
                                for kid in &header.data.kids {
                                    kid_count += 1;
                                    rows.push(vec![
                                        BasicPropertyValue::from(format!("KID {kid_count}")),
                                        BasicPropertyValue::from(""),
                                    ]);
                                    push_row(&mut rows, "algid", kid.algid.as_ref());
                                    push_row(&mut rows, "checksum", kid.checksum.as_ref());
                                    push_row(&mut rows, "kid", kid.value.as_ref());
                                }
                                if let Some(protect_info) = &header.data.protect_info {
                                    for kid in &protect_info.kids {
                                        kid_count += 1;
                                        rows.push(vec![
                                            BasicPropertyValue::from(format!("KID {kid_count}")),
                                            BasicPropertyValue::from(""),
                                        ]);
                                        push_row(
                                            &mut rows,
                                            "algid",
                                            protect_info.algid.as_ref().or(kid.algid.as_ref()),
                                        );
                                        push_row(&mut rows, "keylen", protect_info.keylen);
                                        push_row(&mut rows, "checksum", kid.checksum.as_ref());
                                        push_row(&mut rows, "kid", kid.value.as_ref());
                                    }
                                }
                                push_row(&mut rows, "checksum", header.data.checksum.as_ref());
                                push_row(&mut rows, "la_url", header.data.la_url.as_ref());
                                push_row(&mut rows, "lui_url", header.data.lui_url.as_ref());
                                push_row(&mut rows, "ds_id", header.data.ds_id.as_ref());
                                push_row(
                                    &mut rows,
                                    "custom_attributes",
                                    header.data.custom_attributes.as_ref(),
                                );
                                push_row(
                                    &mut rows,
                                    "decryptor_setup",
                                    header.data.decryptor_setup.as_ref(),
                                );
                            }
                            AtomPropertyValue::Table(TablePropertyValue {
                                headers: None,
                                rows,
                            })
                        }
                        Some(PsshData::Widevine(data)) => {
                            let mut rows = Vec::new();
                            rows.extend(data.key_ids.iter().enumerate().map(|(index, kid)| {
                                vec![
                                    BasicPropertyValue::from(format!("key_id {index}")),
                                    BasicPropertyValue::from(encode_hex(kid)),
                                ]
                            }));
                            if let Some(ref content_id) = data.content_id {
                                rows.push(vec![
                                    BasicPropertyValue::from("content_id"),
                                    BasicPropertyValue::from(
                                        String::from_utf8_lossy(content_id).as_ref(),
                                    ),
                                ]);
                            }
                            if let Some(crypto_period_index) = data.crypto_period_index {
                                rows.push(vec![
                                    BasicPropertyValue::from("crypto_period_index"),
                                    BasicPropertyValue::from(crypto_period_index),
                                ]);
                            }
                            if let Some(protection_scheme) = data.protection_scheme {
                                rows.push(vec![
                                    BasicPropertyValue::from("protection_scheme"),
                                    match protection_scheme {
                                        0 => BasicPropertyValue::from("Unspecified"),
                                        1667591779 => BasicPropertyValue::from("CENC"),
                                        1667392305 => BasicPropertyValue::from("CBC1"),
                                        1667591795 => BasicPropertyValue::from("CENS"),
                                        1667392371 => BasicPropertyValue::from("CBCS"),
                                        n => BasicPropertyValue::from(format!("Unknown: {n}")),
                                    },
                                ]);
                            }
                            if let Some(crypto_period_seconds) = data.crypto_period_seconds {
                                rows.push(vec![
                                    BasicPropertyValue::from("crypto_period_seconds"),
                                    BasicPropertyValue::from(crypto_period_seconds),
                                ]);
                            }
                            if data.type_.is_some() {
                                rows.push(vec![
                                    BasicPropertyValue::from("type"),
                                    match data.type_() {
                                        Type::SINGLE => BasicPropertyValue::from("SINGLE"),
                                        Type::ENTITLEMENT => {
                                            BasicPropertyValue::from("ENTITLEMENT")
                                        }
                                        Type::ENTITLED_KEY => {
                                            BasicPropertyValue::from("ENTITLED_KEY")
                                        }
                                    },
                                ]);
                            }
                            if let Some(key_sequence) = data.key_sequence {
                                rows.push(vec![
                                    BasicPropertyValue::from("key_sequence"),
                                    BasicPropertyValue::from(key_sequence),
                                ]);
                            }
                            let mut group_id_count = 0;
                            for group_id in &data.group_ids {
                                group_id_count += 1;
                                rows.push(vec![
                                    BasicPropertyValue::from(format!("group_id {group_id_count}")),
                                    BasicPropertyValue::from(
                                        String::from_utf8_lossy(group_id).as_ref(),
                                    ),
                                ]);
                            }
                            let mut entitled_keys_count = 0;
                            for entitled_key in &data.entitled_keys {
                                entitled_keys_count += 1;
                                rows.push(vec![
                                    BasicPropertyValue::from(format!(
                                        "entitled_key {entitled_keys_count}"
                                    )),
                                    BasicPropertyValue::from(""),
                                ]);
                                if let Some(id) = &entitled_key.entitlement_key_id {
                                    rows.push(vec![
                                        BasicPropertyValue::from("entitlement_key_id"),
                                        BasicPropertyValue::from(
                                            String::from_utf8_lossy(id).as_ref(),
                                        ),
                                    ]);
                                }
                                if let Some(id) = &entitled_key.key_id {
                                    rows.push(vec![
                                        BasicPropertyValue::from("key_id"),
                                        BasicPropertyValue::from(
                                            String::from_utf8_lossy(id).as_ref(),
                                        ),
                                    ]);
                                }
                                if let Some(id) = &entitled_key.key {
                                    rows.push(vec![
                                        BasicPropertyValue::from("key"),
                                        BasicPropertyValue::from(
                                            String::from_utf8_lossy(id).as_ref(),
                                        ),
                                    ]);
                                }
                                if let Some(iv) = &entitled_key.iv {
                                    rows.push(vec![
                                        BasicPropertyValue::from("iv"),
                                        BasicPropertyValue::from(
                                            String::from_utf8_lossy(iv).as_ref(),
                                        ),
                                    ]);
                                }
                                if let Some(size) = entitled_key.entitlement_key_size_bytes {
                                    rows.push(vec![
                                        BasicPropertyValue::from("entitlement_key_size_bytes"),
                                        BasicPropertyValue::from(size),
                                    ]);
                                }
                            }
                            if let Some(ref feature) = data.video_feature {
                                rows.push(vec![
                                    BasicPropertyValue::from("video_feature"),
                                    BasicPropertyValue::from(feature),
                                ]);
                            }
                            if data.algorithm.is_some() {
                                rows.push(vec![
                                    BasicPropertyValue::from("algorithm"),
                                    match data.algorithm() {
                                        Algorithm::UNENCRYPTED => {
                                            BasicPropertyValue::from("UNENCRYPTED")
                                        }
                                        Algorithm::AESCTR => BasicPropertyValue::from("AESCTR"),
                                    },
                                ]);
                            }
                            if let Some(ref provider) = data.provider {
                                rows.push(vec![
                                    BasicPropertyValue::from("provider"),
                                    BasicPropertyValue::from(provider),
                                ]);
                            }
                            if let Some(ref track_type) = data.track_type {
                                rows.push(vec![
                                    BasicPropertyValue::from("track_type"),
                                    BasicPropertyValue::from(track_type),
                                ]);
                            }
                            if let Some(ref policy) = data.policy {
                                rows.push(vec![
                                    BasicPropertyValue::from("policy"),
                                    BasicPropertyValue::from(policy),
                                ]);
                            }
                            if let Some(grouped_license) = &data.grouped_license {
                                rows.push(vec![
                                    BasicPropertyValue::from("grouped_license"),
                                    BasicPropertyValue::from(
                                        String::from_utf8_lossy(grouped_license).as_ref(),
                                    ),
                                ]);
                            }
                            AtomPropertyValue::Table(TablePropertyValue {
                                headers: None,
                                rows,
                            })
                        }
                        Some(PsshData::Raw(data)) => {
                            AtomPropertyValue::from(BasicPropertyValue::Hex(data.to_owned()))
                        }
                        None => AtomPropertyValue::from(String::new()),
                    },
                ),
            ],
        }
    }
}

fn push_row<K, V>(rows: &mut Vec<Vec<BasicPropertyValue>>, key: K, value: Option<V>)
where
    BasicPropertyValue: From<K>,
    BasicPropertyValue: From<V>,
{
    if let Some(v) = value {
        rows.push(vec![
            BasicPropertyValue::from(key),
            BasicPropertyValue::from(v),
        ]);
    }
}
