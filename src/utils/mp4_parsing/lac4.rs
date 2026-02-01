use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// AC4PresentationLabelBox, ETSI TS 103 190-2 V1.3.1 (2025-07) Sect E.5a
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lac4 {
    pub language_tag: String,
    pub labels: Vec<Ac4PresentationLabel>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ac4PresentationLabel {
    pub id: u16,
    pub label: String,
}
impl Atom for Lac4 {
    const KIND: FourCC = FourCC::new(b"lac4");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let _ = u32::decode(buf)?; // version & flags not used
        let num_presentation_labels = u16::decode(buf)?;
        let language_tag = String::decode(buf)?;
        let mut labels = Vec::with_capacity(usize::from(num_presentation_labels));
        for _ in 0..num_presentation_labels {
            let id = u16::decode(buf)?;
            let label = String::decode(buf)?;
            labels.push(Ac4PresentationLabel { id, label });
        }
        Ok(Self {
            language_tag,
            labels,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}
