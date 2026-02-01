use crate::utils::hex::encode_hex;
use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// SampleEncryptionBox, ISO/IEC 23001-7:2016 Sect 7.2.1
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Senc {
    pub entries: Vec<SencEntry>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SencEntry {
    pub initialization_vector: String,
    pub subsample_encryption: Vec<SencSubsampleEntry>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SencSubsampleEntry {
    pub bytes_of_clear_data: u16,
    pub bytes_of_protected_data: u32,
}
impl Senc {
    pub const UNKNOWN_IV_SIZE: &str = "IV Size";
}
impl Atom for Senc {
    const KIND: FourCC = FourCC::new(b"senc");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let ext = u32::decode(buf)?;
        let use_sub_sample_encryption = ext & 0x2 == 0b10;
        let sample_count = u32::decode(buf)? as usize;
        if sample_count > 4096 {
            return Err(mp4_atom::Error::OutOfMemory);
        }
        if use_sub_sample_encryption {
            // If we are using subsample encryption, then we can't really know what the
            // Per_Sample_IV_Size is, so we try with 0 first then 8 then 16, since those are the
            // only sizes defined. If not any of those then we just fail. In reality, we should be
            // getting this value from somewhere like the `tenc`; however, we don't support
            // depending on another box, so we're making best efforts here (tenc would be
            // particularly awkward because that is in the init segment while this would be in one
            // of the media segments).
            let mut entries = Vec::with_capacity(sample_count);
            // I'm allowing this clippy lint, because if I chain the last 2 else if blocks with an
            // ||, I think that is actually less readable.
            #[allow(clippy::if_same_then_else)]
            if decode_senc_entries_with_subsamples(
                // Because we are going to be trying to decode the buffer multiple times, we don't
                // want to consume the bytes each time, as then subsequent decodes will fail (over
                // decode). Therefore, we copy the remaining data for each decode, so each time
                // there is a fresh copy of the original data.
                &mut buf.slice(buf.remaining()),
                sample_count,
                &mut entries,
                |_| Ok(String::from("0")),
            )
            .is_ok()
            {
                // Since the decoding happened on a copy of the original buffer, it has not been
                // advanced, so we must advance it now. We know it is safe to do so as we have
                // already validated the correct number of bytes were used in the successful decode
                // of the entries.
                buf.advance(buf.remaining());
                Ok(Self { entries })
            } else if decode_senc_entries_with_subsamples(
                &mut buf.slice(buf.remaining()),
                sample_count,
                &mut entries,
                |buf| {
                    Ok(format!(
                        "0x{}",
                        encode_hex(&u64::decode(buf)?.to_be_bytes())
                    ))
                },
            )
            .is_ok()
            {
                buf.advance(buf.remaining());
                Ok(Self { entries })
            } else if decode_senc_entries_with_subsamples(
                &mut buf.slice(buf.remaining()),
                sample_count,
                &mut entries,
                |buf| {
                    Ok(format!(
                        "0x{}",
                        encode_hex(&u128::from_be_bytes(<[u8; 16]>::decode(buf)?).to_be_bytes())
                    ))
                },
            )
            .is_ok()
            {
                buf.advance(buf.remaining());
                Ok(Self { entries })
            } else {
                Err(mp4_atom::Error::Unsupported(Self::UNKNOWN_IV_SIZE))
            }
        } else {
            // If we aren't using subsample encryption, then we can deduce the size of the IV based
            // on how many bytes are left and the sample_count (it must divide exactly).
            let entries = decode_senc_entries_no_subsamples(buf, sample_count)?;
            Ok(Self { entries })
        }
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}
fn decode_senc_entries_no_subsamples<B: Buf>(
    buf: &mut B,
    sample_count: usize,
) -> Result<Vec<SencEntry>> {
    let mut entries = Vec::with_capacity(sample_count);
    let iv_size = buf.remaining() / sample_count;
    match iv_size {
        0 => Ok(Vec::new()),
        8 => {
            for _ in 0..sample_count {
                let iv = u64::decode(buf)?;
                entries.push(SencEntry {
                    initialization_vector: format!("0x{}", encode_hex(&iv.to_be_bytes())),
                    subsample_encryption: Vec::new(),
                });
            }
            Ok(entries)
        }
        16 => {
            for _ in 0..sample_count {
                let iv = u128::from_be_bytes(<[u8; 16]>::decode(buf)?);
                entries.push(SencEntry {
                    initialization_vector: format!("0x{}", encode_hex(&iv.to_be_bytes())),
                    subsample_encryption: Vec::new(),
                });
            }
            Ok(entries)
        }
        _ => Err(mp4_atom::Error::Unsupported(Senc::UNKNOWN_IV_SIZE)),
    }
}
fn decode_senc_entries_with_subsamples<B, F>(
    buf: &mut B,
    sample_count: usize,
    entries: &mut Vec<SencEntry>,
    mut iv_string: F,
) -> Result<()>
where
    B: Buf,
    F: FnMut(&mut B) -> Result<String>,
{
    entries.clear();
    for _ in 0..sample_count {
        let initialization_vector = iv_string(buf)?;
        let subsample_encryption = decode_senc_subsamples(buf)?;
        entries.push(SencEntry {
            initialization_vector,
            subsample_encryption,
        });
    }
    if buf.has_remaining() {
        Err(mp4_atom::Error::UnderDecode(Senc::KIND))
    } else {
        Ok(())
    }
}
fn decode_senc_subsamples<B: Buf>(buf: &mut B) -> Result<Vec<SencSubsampleEntry>> {
    let subsample_count = u16::decode(buf)?;
    if subsample_count > 4096 {
        return Err(mp4_atom::Error::OutOfMemory);
    }
    let mut subsample_encryption = Vec::with_capacity(usize::from(subsample_count));
    for _ in 0..subsample_count {
        let bytes_of_clear_data = u16::decode(buf)?;
        let bytes_of_protected_data = u32::decode(buf)?;
        subsample_encryption.push(SencSubsampleEntry {
            bytes_of_clear_data,
            bytes_of_protected_data,
        });
    }
    Ok(subsample_encryption)
}
