use mp4_atom::{Atom, Buf, BufMut, Decode, FourCC, Result};

/// MetadataDatatypeDefinitionBox, QuickTime:
/// https://developer.apple.com/documentation/quicktime-file-format/metadata_datatype_definition_atom
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dtyp {
    pub data_namespace: u32,
    pub data_value: Vec<u8>,
}

impl Atom for Dtyp {
    const KIND: FourCC = FourCC::new(b"dtyp");

    fn decode_body<B: Buf>(buf: &mut B) -> Result<Self> {
        let data_namespace = u32::decode(buf)?;
        let data_value = Vec::decode(buf)?;
        Ok(Self {
            data_namespace,
            data_value,
        })
    }

    fn encode_body<B: BufMut>(&self, _: &mut B) -> Result<()> {
        unimplemented!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DtypArrayValue<'a> {
    WellKnownType(WellKnownType),
    ReverseAddress(&'a str),
    Unknown(&'a [u8]),
}

impl<'a> From<&'a Dtyp> for DtypArrayValue<'a> {
    fn from(value: &'a Dtyp) -> Self {
        if value.data_namespace == 0 {
            if value.data_value.len() == 4 {
                let code = u32::from_be_bytes([
                    value.data_value[0],
                    value.data_value[1],
                    value.data_value[2],
                    value.data_value[3],
                ]);
                return Self::WellKnownType(WellKnownType::from(code));
            }
        } else if value.data_namespace == 1 && let Ok(s) = std::str::from_utf8(&value.data_value) {
            return Self::ReverseAddress(s);
        }
        Self::Unknown(&value.data_value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum WellKnownType {
    Reserved,
    Utf8,
    Utf16,
    SJis,
    Utf8Sort,
    Utf16Sort,
    Jpeg,
    Png,
    BeSignedInteger,
    BeUnsignedInteger,
    BeFloat32,
    BeFloat64,
    Bmp,
    QuicktimeMetadataAtom,
    ByteSignedInteger,
    Be16BitSignedInteger,
    Be32BitSignedInteger,
    BePointF32,
    BeDimensionsF32,
    BeRectF32,
    Be64BitSignedInteger,
    ByteUnsignedInteger,
    Be16BitUnsignedInteger,
    Be32BitUnsignedInteger,
    Be64BitUnsignedInteger,
    AffineTransformF64,
    Unknown(u32),
}

impl WellKnownType {
    pub fn code(&self) -> u32 {
        match self {
            Self::Reserved => 0,
            Self::Utf8 => 1,
            Self::Utf16 => 2,
            Self::SJis => 3,
            Self::Utf8Sort => 4,
            Self::Utf16Sort => 5,
            Self::Jpeg => 13,
            Self::Png => 14,
            Self::BeSignedInteger => 21,
            Self::BeUnsignedInteger => 22,
            Self::BeFloat32 => 23,
            Self::BeFloat64 => 24,
            Self::Bmp => 27,
            Self::QuicktimeMetadataAtom => 28,
            Self::ByteSignedInteger => 65,
            Self::Be16BitSignedInteger => 66,
            Self::Be32BitSignedInteger => 67,
            Self::BePointF32 => 70,
            Self::BeDimensionsF32 => 71,
            Self::BeRectF32 => 72,
            Self::Be64BitSignedInteger => 74,
            Self::ByteUnsignedInteger => 75,
            Self::Be16BitUnsignedInteger => 76,
            Self::Be32BitUnsignedInteger => 77,
            Self::Be64BitUnsignedInteger => 78,
            Self::AffineTransformF64 => 79,
            Self::Unknown(n) => *n,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Reserved => "RESERVED",
            Self::Utf8 => "UTF-8",
            Self::Utf16 => "UTF-16",
            Self::SJis => "S/JIS",
            Self::Utf8Sort => "UTF-8 sort",
            Self::Utf16Sort => "UTF-16 sort",
            Self::Jpeg => "JPEG",
            Self::Png => "PNG",
            Self::BeSignedInteger => "BE Signed Integer",
            Self::BeUnsignedInteger => "BE Unsigned Integer",
            Self::BeFloat32 => "BE Float32",
            Self::BeFloat64 => "BE Float64",
            Self::Bmp => "BMP",
            Self::QuicktimeMetadataAtom => "QuickTime Metadata atom",
            Self::ByteSignedInteger => "8-bit Signed Integer",
            Self::Be16BitSignedInteger => "BE 16-bit Signed Integer",
            Self::Be32BitSignedInteger => "BE 32-bit Signed Integer",
            Self::BePointF32 => "BE PointF32",
            Self::BeDimensionsF32 => "BE DimensionsF32",
            Self::BeRectF32 => "BE RectF32",
            Self::Be64BitSignedInteger => "BE 64-bit Signed Integer",
            Self::ByteUnsignedInteger => "8-bit Unsigned Integer",
            Self::Be16BitUnsignedInteger => "BE 16-bit Unsigned Integer",
            Self::Be32BitUnsignedInteger => "BE 32-bit Unsigned Integer",
            Self::Be64BitUnsignedInteger => "BE 64-bit Unsigned Integer",
            Self::AffineTransformF64 => "AffineTransformF64",
            Self::Unknown(_) => "Unknown type",
        }
    }

    pub fn comment(&self) -> &'static str {
        match self {
            Self::Reserved => "Reserved for use where no type needs to be indicated",
            Self::Utf8 => "Without any count or NULL terminator",
            Self::Utf16 => "Also known as UTF-16BE",
            Self::SJis => "Deprecated unless it is needed for special Japanese characters",
            Self::Utf8Sort => "Variant storage of a string for sorting only",
            Self::Utf16Sort => "Variant storage of a string for sorting only",
            Self::Jpeg => "In a JFIF wrapper",
            Self::Png => "In a PNG wrapper",
            Self::BeSignedInteger => concat!(
                "A big-endian signed integer in 1,2,3 or 4 bytes. Note: This data type is not ",
                "supported in Timed metadata media. Use one of the fixed-size signed integer data ",
                "types (that is, type codes 65, 66, or 67) instead.",
            ),
            Self::BeUnsignedInteger => concat!(
                "A big-endian unsigned integer in 1,2,3 or 4 bytes; size of value determines ",
                "integer size. Note: This data type is not supported in Timed metadata media. Use ",
                "one of the fixed-size unsigned integer data types (that is, type codes 75, 76, ",
                "or 77) instead.",
            ),
            Self::BeFloat32 => "A big-endian 32-bit floating point value (IEEE754)",
            Self::BeFloat64 => "A big-endian 64-bit floating point value (IEEE754)",
            Self::Bmp => "Windows bitmap format graphics",
            Self::QuicktimeMetadataAtom => concat!(
                "A block of data having the structure of the Metadata atom defined in this ",
                "specification",
            ),
            Self::ByteSignedInteger => "An 8-bit signed integer",
            Self::Be16BitSignedInteger => "A big-endian 16-bit signed integer",
            Self::Be32BitSignedInteger => "A big-endian 32-bit signed integer",
            Self::BePointF32 => concat!(
                "A block of data representing a two dimensional (2D) point with 32-bit big-endian ",
                "floating point x and y coordinates. It has the structure: struct { BEFloat32 x; ",
                "BEFloat32 y; }",
            ),
            Self::BeDimensionsF32 => concat!(
                "A block of data representing 2D dimensions with 32-bit big-endian floating point ",
                "width and height. It has the structure: struct { BEFloat32 width; BEFloat32 ",
                "height; }",
            ),
            Self::BeRectF32 => concat!(
                "A block of data representing a 2D rectangle with 32-bit big-endian floating ",
                "point x and y coordinates and a 32-bit big-endian floating point width and ",
                "height size. It has the structure: struct { BEFloat32 x; BEFloat32 y; BEFloat32 ",
                "width; BEFloat32 height;} or the equivalent structure: struct { PointF32 origin; ",
                "DimensionsF32 size; }",
            ),
            Self::Be64BitSignedInteger => "A big-endian 64-bit signed integer",
            Self::ByteUnsignedInteger => "An 8-bit unsigned integer",
            Self::Be16BitUnsignedInteger => "A big-endian 16-bit unsigned integer",
            Self::Be32BitUnsignedInteger => "A big-endian 32-bit unsigned integer",
            Self::Be64BitUnsignedInteger => "A big-endian 64-bit unsigned integer",
            Self::AffineTransformF64 => concat!(
                "A block of data representing a 3x3 transformation matrix. It has the structure: ",
                "struct { BEFloat64 matrix[3][3]; }",
            ),
            Self::Unknown(_) => concat!(
                "Unknown data code per table found on https://developer.apple.com/documentation/qu",
                "icktime-file-format/well-known_types",
            ),
        }
    }
}

impl From<u32> for WellKnownType {
    fn from(value: u32) -> Self {
        match value {
            0 => Self::Reserved,
            1 => Self::Utf8,
            2 => Self::Utf16,
            3 => Self::SJis,
            4 => Self::Utf8Sort,
            5 => Self::Utf16Sort,
            13 => Self::Jpeg,
            14 => Self::Png,
            21 => Self::BeSignedInteger,
            22 => Self::BeUnsignedInteger,
            23 => Self::BeFloat32,
            24 => Self::BeFloat64,
            27 => Self::Bmp,
            28 => Self::QuicktimeMetadataAtom,
            65 => Self::ByteSignedInteger,
            66 => Self::Be16BitSignedInteger,
            67 => Self::Be32BitSignedInteger,
            70 => Self::BePointF32,
            71 => Self::BeDimensionsF32,
            72 => Self::BeRectF32,
            74 => Self::Be64BitSignedInteger,
            75 => Self::ByteUnsignedInteger,
            76 => Self::Be16BitUnsignedInteger,
            77 => Self::Be32BitUnsignedInteger,
            78 => Self::Be64BitUnsignedInteger,
            79 => Self::AffineTransformF64,
            n => Self::Unknown(n),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::io::Cursor;

    // Example taken from:
    // https://devstreaming-cdn.apple.com/videos/streaming/examples/immersive-media/apple-immersive-video/primary.m3u8
    const DTYP: &[u8] = &[
        0x00, 0x00, 0x00, 0x10, 0x64, 0x74, 0x79, 0x70, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00,
    ];

    #[test]
    fn parses_correctly() {
        let mut buf = Cursor::new(DTYP);
        assert_eq!(
            Dtyp {
                data_namespace: 0,
                data_value: vec![0, 0, 0, 0],
            },
            Dtyp::decode(&mut buf).expect("dtyp should decode successfully")
        );
    }
}
