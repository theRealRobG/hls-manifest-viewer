use crate::utils::{
    mp4_atom_properties::{
        AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue,
        TablePropertyValue,
    },
    mp4_parsing::{
        dac4::{
            Ac4BitrateMode, Ac4ContentClassifier, Ac4Presentation, Ac4PresentationV0,
            Ac4PresentationV1, Ac4SubstreamGroup, EmdfSubstream,
        },
        Dac4,
    },
};
use std::{borrow::Cow, fmt::Display};

impl AtomWithProperties for Dac4 {
    fn properties(&self) -> AtomProperties {
        let box_name = "AC4SpecificBox";
        let mut properties = Vec::new();

        properties.push((
            Cow::Borrowed("ac4_dsi_version"),
            AtomPropertyValue::from(self.ac4_dsi_version),
        ));
        properties.push((
            Cow::Borrowed("bitstream_version"),
            AtomPropertyValue::from(self.bitstream_version),
        ));
        properties.push((
            Cow::Borrowed("fs_index"),
            AtomPropertyValue::from(if self.fs_index { 1 } else { 0 }),
        ));
        properties.push((
            Cow::Borrowed("frame_rate_index"),
            AtomPropertyValue::from(self.frame_rate_index),
        ));
        if let Some(short_program_id) = self.short_program_id {
            properties.push((
                Cow::Borrowed("short_program_id"),
                AtomPropertyValue::from(short_program_id),
            ));
        }
        if let Some(program_uuid) = self.program_uuid {
            properties.push((
                Cow::Borrowed("program_uuid"),
                AtomPropertyValue::from(String::from_utf8_lossy(&program_uuid).as_ref()),
            ));
        }
        properties.push((
            Cow::Borrowed("bit_rate_mode"),
            AtomPropertyValue::from(format!("{}", self.bit_rate_mode)),
        ));
        properties.push((
            Cow::Borrowed("bit_rate"),
            AtomPropertyValue::from(self.bit_rate),
        ));
        properties.push((
            Cow::Borrowed("bit_rate_precision"),
            AtomPropertyValue::from(self.bit_rate_precision),
        ));

        for (i, presentation) in self.presentations.iter().enumerate() {
            let key = Cow::Owned(format!("presentation #{}", i + 1));
            let table = match presentation {
                Ac4Presentation::V0(p) => presentation_v0(p),
                Ac4Presentation::V1(p) => presentation_v1(p),
                Ac4Presentation::V2(p) => presentation_v2(p),
                Ac4Presentation::UnknownVersion(v) => TablePropertyValue {
                    headers: None,
                    rows: vec![vec![
                        BasicPropertyValue::from("unhandled version"),
                        BasicPropertyValue::from(*v),
                    ]],
                },
            };
            properties.push((key, AtomPropertyValue::Table(table)));
        }

        AtomProperties {
            box_name,
            properties,
        }
    }
}

fn presentation_v0(p: &Ac4PresentationV0) -> TablePropertyValue {
    let mut rows: Vec<Vec<BasicPropertyValue>> = Vec::new();

    rows.push(vec!["version".into(), 0.into()]);
    rows.push(vec!["config".into(), p.presentation_config.into()]);
    if let Some(md_compat) = p.md_compat {
        rows.push(vec!["md_compat".into(), md_compat.into()]);
    }
    if let Some(presentation_id) = p.presentation_id {
        rows.push(vec!["id".into(), presentation_id.into()])
    }
    if let Some(dsi_frame_rate_multiply_info) = p.dsi_frame_rate_multiply_info {
        rows.push(vec![
            "dsi_frame_rate_multiply_info".into(),
            dsi_frame_rate_multiply_info.into(),
        ])
    }
    if let Some(presentation_emdf_version) = p.presentation_emdf_version {
        rows.push(vec![
            "emdf_version".into(),
            presentation_emdf_version.into(),
        ])
    }
    if let Some(presentation_key_id) = p.presentation_key_id {
        rows.push(vec!["key_id".into(), presentation_key_id.into()])
    }
    if let Some(presentation_channel_mask) = p.presentation_channel_mask {
        rows.push(vec![
            "channel_mask".into(),
            BasicPropertyValue::BinaryMask(presentation_channel_mask.to_vec()),
        ])
    }
    if let Some(b_hsf_ext) = p.b_hsf_ext {
        rows.push(vec!["hsf_ext".into(), b_hsf_ext.into()])
    }
    if let Some(groups) = &p.substream_groups {
        substream_groups(&mut rows, groups);
    }
    if let Some(b_pre_virtualized) = p.b_pre_virtualized {
        rows.push(vec!["pre_virtualized".into(), b_pre_virtualized.into()])
    }
    emdf_substreams(&mut rows, &p.emdf_substreams);

    TablePropertyValue {
        headers: None,
        rows,
    }
}

fn presentation_v1(p: &Ac4PresentationV1) -> TablePropertyValue {
    let mut rows: Vec<Vec<BasicPropertyValue>> = Vec::new();

    rows.push(vec!["version".into(), 1.into()]);
    presentation_v1_v2_common(&mut rows, p);
    if let Some(immersive_audio_indicator) = p.immersive_audio_indicator {
        rows.push(vec![
            "immersive_audio".into(),
            format!("{immersive_audio_indicator}").into(),
        ]);
    }
    if let Some(extended_presentation_id) = p.extended_presentation_id {
        rows.push(vec![
            "extended_id".into(),
            format!("{extended_presentation_id}").into(),
        ]);
    }

    TablePropertyValue {
        headers: None,
        rows,
    }
}

fn presentation_v2(p: &Ac4PresentationV1) -> TablePropertyValue {
    let mut rows: Vec<Vec<BasicPropertyValue>> = Vec::new();

    rows.push(vec!["version".into(), 2.into()]);
    presentation_v1_v2_common(&mut rows, p);
    if let Some(immersive_audio_indicator) = p.immersive_audio_indicator {
        // immersive_audio_indicator renamed to dolby_atmos_indicator in v2:
        // https://media.developer.dolby.com/AC4/AC4_DASH_for_BROADCAST_SPEC.pdf
        rows.push(vec![
            "dolby_atmos".into(),
            format!("{immersive_audio_indicator}").into(),
        ]);
    }
    if let Some(extended_presentation_id) = p.extended_presentation_id {
        rows.push(vec![
            "extended_id".into(),
            format!("{extended_presentation_id}").into(),
        ]);
    }

    TablePropertyValue {
        headers: None,
        rows,
    }
}

fn presentation_v1_v2_common(rows: &mut Vec<Vec<BasicPropertyValue>>, p: &Ac4PresentationV1) {
    rows.push(vec!["config".into(), p.presentation_config_v1.into()]);
    if let Some(md_compat) = p.md_compat {
        rows.push(vec!["md_compat".into(), md_compat.into()]);
    }
    if let Some(presentation_id) = p.presentation_id {
        rows.push(vec!["id".into(), presentation_id.into()]);
    }
    if let Some(dsi_frame_rate_multiply_info) = p.dsi_frame_rate_multiply_info {
        rows.push(vec![
            "dsi_frame_rate_multiply_info".into(),
            dsi_frame_rate_multiply_info.into(),
        ]);
    }
    if let Some(dsi_frame_rate_fraction_info) = p.dsi_frame_rate_fraction_info {
        rows.push(vec![
            "dsi_frame_rate_fraction_info".into(),
            dsi_frame_rate_fraction_info.into(),
        ]);
    }
    if let Some(presentation_emdf_version) = p.presentation_emdf_version {
        rows.push(vec![
            "emdf_version".into(),
            presentation_emdf_version.into(),
        ]);
    }
    if let Some(presentation_key_id) = p.presentation_key_id {
        rows.push(vec!["key_id".into(), presentation_key_id.into()]);
    }
    if let Some(b_presentation_channel_coded) = p.b_presentation_channel_coded {
        rows.push(vec![
            "channel_coded".into(),
            b_presentation_channel_coded.into(),
        ]);
    }
    if let Some(dsi_presentation_ch_mode) = p.dsi_presentation_ch_mode {
        rows.push(vec!["ch_mode".into(), dsi_presentation_ch_mode.into()]);
    }
    if let Some(pres_b_4_back_channels_present) = p.pres_b_4_back_channels_present {
        rows.push(vec![
            "4_back_channels".into(),
            pres_b_4_back_channels_present.into(),
        ]);
    }
    if let Some(pres_top_channel_pairs) = p.pres_top_channel_pairs {
        rows.push(vec![
            "top_channel_pairs".into(),
            pres_top_channel_pairs.into(),
        ]);
    }
    if let Some(presentation_channel_mask_v1) = p.presentation_channel_mask_v1 {
        rows.push(vec![
            "channel_mask".into(),
            BasicPropertyValue::BinaryMask(presentation_channel_mask_v1.to_vec()),
        ]);
    }
    if let Some(b_presentation_core_differs) = p.b_presentation_core_differs {
        rows.push(vec![
            "core_differs".into(),
            b_presentation_core_differs.into(),
        ]);
    }
    if let Some(b_presentation_core_channel_coded) = p.b_presentation_core_channel_coded {
        rows.push(vec![
            "core_channel_coded".into(),
            b_presentation_core_channel_coded.into(),
        ]);
    }
    if let Some(dsi_presentation_channel_mode_core) = p.dsi_presentation_channel_mode_core {
        rows.push(vec![
            "channel_mode_core".into(),
            dsi_presentation_channel_mode_core.into(),
        ]);
    }
    if let Some(b_presentation_filter) = p.b_presentation_filter {
        rows.push(vec![
            "presentation_filter".into(),
            b_presentation_filter.into(),
        ]);
    }
    if let Some(b_enable_presentation) = p.b_enable_presentation {
        rows.push(vec![
            "enable_presentation".into(),
            b_enable_presentation.into(),
        ]);
    }
    if let Some(filter_data) = &p.filter_data {
        rows.push(vec![
            "filter_data".into(),
            BasicPropertyValue::Hex(filter_data.clone()),
        ]);
    }
    if let Some(b_multi_pid) = p.b_multi_pid {
        rows.push(vec!["multi_pid".into(), b_multi_pid.into()])
    }
    if let Some(groups) = &p.substream_groups {
        substream_groups(rows, groups);
    }
    if let Some(b_pre_virtualized) = p.b_pre_virtualized {
        rows.push(vec!["pre_virtualized".into(), b_pre_virtualized.into()])
    }
    emdf_substreams(rows, &p.emdf_substreams);
    if let Some(bit_rate_mode) = p.bit_rate_mode {
        rows.push(vec![
            "bit_rate_mode".into(),
            format!("{bit_rate_mode}").into(),
        ]);
    }
    if let Some(bit_rate) = p.bit_rate {
        rows.push(vec!["bit_rate".into(), bit_rate.into()])
    }
    if let Some(bit_rate_precision) = p.bit_rate_precision {
        rows.push(vec!["bit_rate_precision".into(), bit_rate_precision.into()])
    }
    if let Some(alternative_info) = &p.alternative_info {
        rows.push(vec!["alternative_info".into(), "".into()]);
        rows.push(vec![
            "name".into(),
            alternative_info.presentation_name.clone().into(),
        ]);
        if !alternative_info.targets.is_empty() {
            rows.push(vec!["alternative_info_targets".into(), "".into()]);
            for (i, target) in alternative_info.targets.iter().enumerate() {
                rows.push(vec![
                    format!("[{i}] md_compat").into(),
                    target.md_compat.into(),
                ]);
                rows.push(vec![
                    format!("[{i}] device_category").into(),
                    target.device_category.into(),
                ]);
            }
        }
    }
    if let Some(de_indicator) = p.de_indicator {
        rows.push(vec![
            "dialogue_enancement".into(),
            format!("{de_indicator}").into(),
        ]);
    }
}

fn substream_groups(
    rows: &mut Vec<Vec<BasicPropertyValue>>,
    substream_groups: &[Ac4SubstreamGroup],
) {
    if substream_groups.is_empty() {
        rows.push(vec!["substream_groups".into(), "empty".into()]);
    } else {
        rows.push(vec![
            "substream_groups".into(),
            format!("g.len() == {}", substream_groups.len()).into(),
        ]);
    }
    for (i, group) in substream_groups.iter().enumerate() {
        rows.push(vec![
            format!("g[{i}] hsf_ext").into(),
            group.b_hsf_ext.into(),
        ]);
        rows.push(vec![
            format!("g[{i}] channel_coded").into(),
            group.b_channel_coded.into(),
        ]);
        if !group.substreams.is_empty() {
            rows.push(vec![
                format!("g[{i}] substreams").into(),
                format!("s.len() == {}", group.substreams.len()).into(),
            ]);
        }
        for (j, substream) in group.substreams.iter().enumerate() {
            rows.push(vec![
                format!("g[{i}]s[{j}] sf_multiplier").into(),
                substream.dsi_sf_multiplier.into(),
            ]);
            if let Some(bitrate_indicator) = substream.bitrate_indicator {
                rows.push(vec![
                    format!("g[{i}]s[{j}] bitrate_indicator").into(),
                    bitrate_indicator.into(),
                ]);
            }
            if let Some(channel_mask) = substream.channel_mask {
                rows.push(vec![
                    format!("g[{i}]s[{j}] channel_mask").into(),
                    BasicPropertyValue::BinaryMask(channel_mask.to_vec()),
                ]);
            }
            if let Some(n_dmx_objects_minus1) = substream.n_dmx_objects_minus1 {
                rows.push(vec![
                    format!("g[{i}]s[{j}] n_dmx_objects").into(),
                    (n_dmx_objects_minus1 as usize + 1).into(),
                ])
            }
            if let Some(n_umx_objects_minus1) = substream.n_umx_objects_minus1 {
                rows.push(vec![
                    format!("g[{i}]s[{j}] n_umx_objects").into(),
                    (n_umx_objects_minus1 as usize + 1).into(),
                ])
            }
            if let Some(contains_bed_objects) = substream.contains_bed_objects {
                rows.push(vec![
                    format!("g[{i}]s[{j}] contains_bed_objects").into(),
                    contains_bed_objects.into(),
                ])
            }
            if let Some(contains_dynamic_objects) = substream.contains_dynamic_objects {
                rows.push(vec![
                    format!("g[{i}]s[{j}] contains_dynamic_objects").into(),
                    contains_dynamic_objects.into(),
                ])
            }
            if let Some(contains_isf_objects) = substream.contains_isf_objects {
                rows.push(vec![
                    format!("g[{i}]s[{j}] contains_isf_objects").into(),
                    contains_isf_objects.into(),
                ])
            }
        }
        if let Some(content_classifier) = group.content_classifier {
            rows.push(vec![
                format!("g[{i}] content_classifier").into(),
                format!("{content_classifier}").into(),
            ]);
        }
        if let Some(language_tag) = &group.language_tag {
            rows.push(vec![
                format!("g[{i}] language_tag").into(),
                language_tag.into(),
            ]);
        }
    }
}

fn emdf_substreams(rows: &mut Vec<Vec<BasicPropertyValue>>, emdf_substreams: &[EmdfSubstream]) {
    if emdf_substreams.is_empty() {
        return;
    }
    rows.push(vec!["emdf_substreams".into(), "".into()]);
    for (i, substream) in emdf_substreams.iter().enumerate() {
        rows.push(vec![
            format!("[{i}] version").into(),
            substream.emdf_version.into(),
        ]);
        rows.push(vec![
            format!("[{i}] key_id").into(),
            substream.key_id.into(),
        ]);
    }
}

impl Display for Ac4BitrateMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotSpecified => write!(f, "not specified"),
            Self::Constant => write!(f, "constant (1)"),
            Self::Average => write!(f, "average (2)"),
            Self::Variable => write!(f, "variable (3)"),
        }
    }
}

impl Display for Ac4ContentClassifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ac4ContentClassifier::CompleteMain => write!(f, "complete main (0)"),
            Ac4ContentClassifier::MusicAndEffects => write!(f, "music and effects (1)"),
            Ac4ContentClassifier::VisuallyImpaired => write!(f, "visually impaired (2)"),
            Ac4ContentClassifier::HearingImpaired => write!(f, "hearing impaired (3)"),
            Ac4ContentClassifier::Dialogue => write!(f, "dialogue (4)"),
            Ac4ContentClassifier::Commentary => write!(f, "commentary (5)"),
            Ac4ContentClassifier::Emergency => write!(f, "emergency (6)"),
            Ac4ContentClassifier::VoiceOver => write!(f, "voice over (7)"),
        }
    }
}
