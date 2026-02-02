use std::{borrow::Cow, fmt::Display};

use crate::utils::{
    mp4_atom_properties::{
        AtomProperties, AtomPropertyValue, AtomWithProperties, BasicPropertyValue,
        TablePropertyValue,
    },
    mp4_parsing::{
        dec3::{ChanLoc, IndependentSubstream},
        Dec3,
    },
};

impl AtomWithProperties for Dec3 {
    fn properties(&self) -> AtomProperties {
        let box_name = "EC3SpecificBox";
        let mut properties = Vec::new();

        properties.push((
            Cow::Borrowed("data_rate"),
            AtomPropertyValue::from(self.data_rate),
        ));
        for (i, is) in self.independent_substreams.iter().enumerate() {
            let key = Cow::Owned(format!("independent_substream #{}", i + 1));
            let mut rows: Vec<Vec<BasicPropertyValue>> = vec![
                vec!["fscod".into(), is.fscod.into()],
                vec!["bsid".into(), is.bsid.into()],
                vec!["asvc".into(), is.asvc.into()],
                vec!["bsmod".into(), is.bsmod.into()],
                vec!["acmod".into(), is.acmod.into()],
                vec!["lfeon".into(), is.lfeon.into()],
                vec!["num_dep_sub".into(), is.num_dep_sub.into()],
            ];
            if let Some(chan_loc) = pretty_chan_loc(is) {
                rows.push(vec!["chan_loc".into(), chan_loc.into()]);
            }
            properties.push((
                key,
                AtomPropertyValue::Table(TablePropertyValue {
                    headers: None,
                    rows,
                }),
            ));
        }
        if self.independent_substreams.is_empty() {
            properties.push((
                Cow::Borrowed("independent_substreams"),
                AtomPropertyValue::from(0),
            ));
        }

        AtomProperties {
            box_name,
            properties,
        }
    }
}

impl Display for ChanLoc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChanLoc::LcRc => write!(f, "Lc/Rc"),
            ChanLoc::LrsRrs => write!(f, "Lrs/Rrs"),
            ChanLoc::Cs => write!(f, "Cs"),
            ChanLoc::Ts => write!(f, "Ts"),
            ChanLoc::LsdRsd => write!(f, "Lsd/Rsd"),
            ChanLoc::LwRw => write!(f, "Lw/Rw"),
            ChanLoc::LvhRvh => write!(f, "Lvh/Rvh"),
            ChanLoc::Cvh => write!(f, "Cvh"),
            ChanLoc::LFE2 => write!(f, "LFE2"),
        }
    }
}

fn pretty_chan_loc(is: &IndependentSubstream) -> Option<String> {
    let chan_loc = is.chan_loc?;
    Some(format!(
        "{:09b} ({})",
        chan_loc,
        is.descriptive_chan_loc()
            .iter()
            .map(|c| format!("{c}"))
            .collect::<Vec<String>>()
            .join(" + ")
    ))
}
