//! Kiwi binary format decoder.
//!
//! Kiwi is a schema-based binary encoding used inside .fig files.
//! Each message consists of tagged fields. Each field has:
//! - A field ID (varint)
//! - A type tag (varint)
//! - The value (type-dependent encoding)
//!
//! Field ID 0 terminates a message.
//!
//! Supported value types:
//! - Bool:    1 byte (0 or 1)
//! - Int:     varint (zigzag encoded)
//! - UInt:    varint
//! - Float:   4 bytes (f32 LE)
//! - String:  varint length + UTF-8 bytes
//! - Bytes:   varint length + raw bytes
//! - Nested:  recursive message
//! - Array:   varint count + repeated values

use crate::error::{FigmaError, FigmaResult};

/// Type tag for Kiwi fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum KiwiType {
    Bool = 1,
    Int = 2,
    UInt = 3,
    Float = 4,
    String = 5,
    Bytes = 6,
    Nested = 7,
    Array = 8,
}

impl KiwiType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Bool),
            2 => Some(Self::Int),
            3 => Some(Self::UInt),
            4 => Some(Self::Float),
            5 => Some(Self::String),
            6 => Some(Self::Bytes),
            7 => Some(Self::Nested),
            8 => Some(Self::Array),
            _ => None,
        }
    }
}

/// A decoded Kiwi field value.
#[derive(Debug, Clone, PartialEq)]
pub enum KiwiValue {
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f32),
    String(String),
    Bytes(Vec<u8>),
    Nested(Vec<KiwiField>),
    Array(Vec<KiwiValue>),
}

impl KiwiValue {
    /// Extract as bool, or None.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            KiwiValue::Bool(v) => Some(*v),
            _ => None,
        }
    }

    /// Extract as i64, or None.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            KiwiValue::Int(v) => Some(*v),
            _ => None,
        }
    }

    /// Extract as u64, or None.
    pub fn as_uint(&self) -> Option<u64> {
        match self {
            KiwiValue::UInt(v) => Some(*v),
            _ => None,
        }
    }

    /// Extract as f32, or None.
    pub fn as_float(&self) -> Option<f32> {
        match self {
            KiwiValue::Float(v) => Some(*v),
            _ => None,
        }
    }

    /// Extract as string reference, or None.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            KiwiValue::String(v) => Some(v.as_str()),
            _ => None,
        }
    }

    /// Extract as bytes, or None.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            KiwiValue::Bytes(v) => Some(v),
            _ => None,
        }
    }

    /// Extract as nested fields, or None.
    pub fn as_nested(&self) -> Option<&[KiwiField]> {
        match self {
            KiwiValue::Nested(v) => Some(v),
            _ => None,
        }
    }

    /// Extract as array, or None.
    pub fn as_array(&self) -> Option<&[KiwiValue]> {
        match self {
            KiwiValue::Array(v) => Some(v),
            _ => None,
        }
    }
}

/// A decoded field: ID + value.
#[derive(Debug, Clone, PartialEq)]
pub struct KiwiField {
    pub id: u32,
    pub value: KiwiValue,
}

/// Decoder for Kiwi binary format.
pub struct KiwiDecoder<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> KiwiDecoder<'a> {
    /// Create a new decoder over the given byte slice.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Current read position.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Remaining bytes.
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    /// Whether all data has been consumed.
    pub fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    /// Read a single byte.
    pub fn read_byte(&mut self) -> FigmaResult<u8> {
        if self.pos >= self.data.len() {
            return Err(FigmaError::UnexpectedEof(self.pos));
        }
        let b = self.data[self.pos];
        self.pos += 1;
        Ok(b)
    }

    /// Read a bool (1 byte: 0 = false, 1 = true).
    pub fn read_bool(&mut self) -> FigmaResult<bool> {
        let b = self.read_byte()?;
        Ok(b != 0)
    }

    /// Read a varint (unsigned, up to 64 bits).
    pub fn read_varint(&mut self) -> FigmaResult<u64> {
        let mut result: u64 = 0;
        let mut shift = 0u32;
        loop {
            let b = self.read_byte()?;
            result |= ((b & 0x7F) as u64) << shift;
            if b & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift > 63 {
                return Err(FigmaError::ParseError {
                    offset: self.pos,
                    message: "varint too large".into(),
                });
            }
        }
        Ok(result)
    }

    /// Read a signed varint (zigzag encoded).
    pub fn read_signed_varint(&mut self) -> FigmaResult<i64> {
        let v = self.read_varint()?;
        // Zigzag decode: (v >> 1) ^ -(v & 1)
        Ok(((v >> 1) as i64) ^ (-((v & 1) as i64)))
    }

    /// Read a 32-bit float (LE).
    pub fn read_f32(&mut self) -> FigmaResult<f32> {
        if self.pos + 4 > self.data.len() {
            return Err(FigmaError::UnexpectedEof(self.pos));
        }
        let bytes = [
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ];
        self.pos += 4;
        Ok(f32::from_le_bytes(bytes))
    }

    /// Read a length-prefixed UTF-8 string.
    pub fn read_string(&mut self) -> FigmaResult<String> {
        let len = self.read_varint()? as usize;
        if self.pos + len > self.data.len() {
            return Err(FigmaError::UnexpectedEof(self.pos));
        }
        let s = std::str::from_utf8(&self.data[self.pos..self.pos + len]).map_err(|e| {
            FigmaError::ParseError {
                offset: self.pos,
                message: format!("invalid UTF-8: {e}"),
            }
        })?;
        self.pos += len;
        Ok(s.to_string())
    }

    /// Read length-prefixed raw bytes.
    pub fn read_bytes(&mut self) -> FigmaResult<Vec<u8>> {
        let len = self.read_varint()? as usize;
        if self.pos + len > self.data.len() {
            return Err(FigmaError::UnexpectedEof(self.pos));
        }
        let bytes = self.data[self.pos..self.pos + len].to_vec();
        self.pos += len;
        Ok(bytes)
    }

    /// Read a typed value based on the given type tag.
    pub fn read_value(&mut self, type_tag: KiwiType) -> FigmaResult<KiwiValue> {
        match type_tag {
            KiwiType::Bool => Ok(KiwiValue::Bool(self.read_bool()?)),
            KiwiType::Int => Ok(KiwiValue::Int(self.read_signed_varint()?)),
            KiwiType::UInt => Ok(KiwiValue::UInt(self.read_varint()?)),
            KiwiType::Float => Ok(KiwiValue::Float(self.read_f32()?)),
            KiwiType::String => Ok(KiwiValue::String(self.read_string()?)),
            KiwiType::Bytes => Ok(KiwiValue::Bytes(self.read_bytes()?)),
            KiwiType::Nested => Ok(KiwiValue::Nested(self.read_message()?)),
            KiwiType::Array => {
                // Arrays are encoded as: element_type (1 byte) + count (varint) + values
                let elem_type_byte = self.read_byte()?;
                let elem_type = KiwiType::from_u8(elem_type_byte).ok_or_else(|| {
                    FigmaError::ParseError {
                        offset: self.pos - 1,
                        message: format!("unknown array element type: {elem_type_byte}"),
                    }
                })?;
                let count = self.read_varint()? as usize;
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    items.push(self.read_value(elem_type)?);
                }
                Ok(KiwiValue::Array(items))
            }
        }
    }

    /// Read a complete message (sequence of tagged fields until field ID 0).
    pub fn read_message(&mut self) -> FigmaResult<Vec<KiwiField>> {
        let mut fields = Vec::new();
        loop {
            let field_id = self.read_varint()? as u32;
            if field_id == 0 {
                break;
            }
            let type_byte = self.read_byte()?;
            let type_tag = KiwiType::from_u8(type_byte).ok_or_else(|| {
                FigmaError::ParseError {
                    offset: self.pos - 1,
                    message: format!("unknown field type tag: {type_byte}"),
                }
            })?;
            let value = self.read_value(type_tag)?;
            fields.push(KiwiField { id: field_id, value });
        }
        Ok(fields)
    }

    /// Read the entire data as a top-level message.
    pub fn decode_root(&mut self) -> FigmaResult<Vec<KiwiField>> {
        self.read_message()
    }
}

/// Helper: find a field by ID in a list of fields.
pub fn find_field(fields: &[KiwiField], id: u32) -> Option<&KiwiValue> {
    fields.iter().find(|f| f.id == id).map(|f| &f.value)
}

/// Helper: get a string field by ID.
pub fn get_string(fields: &[KiwiField], id: u32) -> Option<&str> {
    find_field(fields, id).and_then(|v| v.as_str())
}

/// Helper: get a float field by ID.
pub fn get_float(fields: &[KiwiField], id: u32) -> Option<f32> {
    find_field(fields, id).and_then(|v| v.as_float())
}

/// Helper: get a uint field by ID.
pub fn get_uint(fields: &[KiwiField], id: u32) -> Option<u64> {
    find_field(fields, id).and_then(|v| v.as_uint())
}

/// Helper: get a bool field by ID.
pub fn get_bool(fields: &[KiwiField], id: u32) -> Option<bool> {
    find_field(fields, id).and_then(|v| v.as_bool())
}

/// Helper: get nested fields by ID.
pub fn get_nested(fields: &[KiwiField], id: u32) -> Option<&[KiwiField]> {
    find_field(fields, id).and_then(|v| v.as_nested())
}

/// Encode a Kiwi message to bytes (for testing and fixture generation).
pub struct KiwiEncoder {
    buf: Vec<u8>,
}

impl KiwiEncoder {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    pub fn write_byte(&mut self, b: u8) {
        self.buf.push(b);
    }

    pub fn write_bool(&mut self, v: bool) {
        self.buf.push(if v { 1 } else { 0 });
    }

    pub fn write_varint(&mut self, mut v: u64) {
        loop {
            let byte = (v & 0x7F) as u8;
            v >>= 7;
            if v == 0 {
                self.buf.push(byte);
                break;
            } else {
                self.buf.push(byte | 0x80);
            }
        }
    }

    pub fn write_signed_varint(&mut self, v: i64) {
        // Zigzag encode: (v << 1) ^ (v >> 63)
        let encoded = ((v << 1) ^ (v >> 63)) as u64;
        self.write_varint(encoded);
    }

    pub fn write_f32(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn write_string(&mut self, s: &str) {
        self.write_varint(s.len() as u64);
        self.buf.extend_from_slice(s.as_bytes());
    }

    pub fn write_bytes_value(&mut self, data: &[u8]) {
        self.write_varint(data.len() as u64);
        self.buf.extend_from_slice(data);
    }

    /// Write a field with ID and type tag, then the value.
    pub fn write_field(&mut self, id: u32, value: &KiwiValue) {
        self.write_varint(id as u64);
        match value {
            KiwiValue::Bool(v) => {
                self.write_byte(KiwiType::Bool as u8);
                self.write_bool(*v);
            }
            KiwiValue::Int(v) => {
                self.write_byte(KiwiType::Int as u8);
                self.write_signed_varint(*v);
            }
            KiwiValue::UInt(v) => {
                self.write_byte(KiwiType::UInt as u8);
                self.write_varint(*v);
            }
            KiwiValue::Float(v) => {
                self.write_byte(KiwiType::Float as u8);
                self.write_f32(*v);
            }
            KiwiValue::String(v) => {
                self.write_byte(KiwiType::String as u8);
                self.write_string(v);
            }
            KiwiValue::Bytes(v) => {
                self.write_byte(KiwiType::Bytes as u8);
                self.write_bytes_value(v);
            }
            KiwiValue::Nested(fields) => {
                self.write_byte(KiwiType::Nested as u8);
                for f in fields {
                    self.write_field(f.id, &f.value);
                }
                self.write_varint(0); // terminator
            }
            KiwiValue::Array(items) => {
                self.write_byte(KiwiType::Array as u8);
                // Determine element type from first item
                if let Some(first) = items.first() {
                    let elem_type = match first {
                        KiwiValue::Bool(_) => KiwiType::Bool,
                        KiwiValue::Int(_) => KiwiType::Int,
                        KiwiValue::UInt(_) => KiwiType::UInt,
                        KiwiValue::Float(_) => KiwiType::Float,
                        KiwiValue::String(_) => KiwiType::String,
                        KiwiValue::Bytes(_) => KiwiType::Bytes,
                        KiwiValue::Nested(_) => KiwiType::Nested,
                        KiwiValue::Array(_) => KiwiType::Array,
                    };
                    self.write_byte(elem_type as u8);
                    self.write_varint(items.len() as u64);
                    for item in items {
                        match item {
                            KiwiValue::Bool(v) => self.write_bool(*v),
                            KiwiValue::Int(v) => self.write_signed_varint(*v),
                            KiwiValue::UInt(v) => self.write_varint(*v),
                            KiwiValue::Float(v) => self.write_f32(*v),
                            KiwiValue::String(v) => self.write_string(v),
                            KiwiValue::Bytes(v) => self.write_bytes_value(v),
                            KiwiValue::Nested(fields) => {
                                for f in fields {
                                    self.write_field(f.id, &f.value);
                                }
                                self.write_varint(0);
                            }
                            KiwiValue::Array(_) => {} // nested arrays not supported
                        }
                    }
                } else {
                    self.write_byte(KiwiType::UInt as u8);
                    self.write_varint(0);
                }
            }
        }
    }

    /// Write field ID 0 to terminate a message.
    pub fn write_terminator(&mut self) {
        self.write_varint(0);
    }
}

impl Default for KiwiEncoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_single_byte() {
        let mut enc = KiwiEncoder::new();
        enc.write_varint(42);
        let data = enc.into_bytes();
        let mut dec = KiwiDecoder::new(&data);
        assert_eq!(dec.read_varint().unwrap(), 42);
    }

    #[test]
    fn test_varint_multi_byte() {
        let mut enc = KiwiEncoder::new();
        enc.write_varint(300);
        let data = enc.into_bytes();
        let mut dec = KiwiDecoder::new(&data);
        assert_eq!(dec.read_varint().unwrap(), 300);
    }

    #[test]
    fn test_varint_large() {
        let mut enc = KiwiEncoder::new();
        enc.write_varint(u64::MAX >> 1);
        let data = enc.into_bytes();
        let mut dec = KiwiDecoder::new(&data);
        assert_eq!(dec.read_varint().unwrap(), u64::MAX >> 1);
    }

    #[test]
    fn test_varint_zero() {
        let mut enc = KiwiEncoder::new();
        enc.write_varint(0);
        let data = enc.into_bytes();
        let mut dec = KiwiDecoder::new(&data);
        assert_eq!(dec.read_varint().unwrap(), 0);
    }

    #[test]
    fn test_signed_varint_positive() {
        let mut enc = KiwiEncoder::new();
        enc.write_signed_varint(42);
        let data = enc.into_bytes();
        let mut dec = KiwiDecoder::new(&data);
        assert_eq!(dec.read_signed_varint().unwrap(), 42);
    }

    #[test]
    fn test_signed_varint_negative() {
        let mut enc = KiwiEncoder::new();
        enc.write_signed_varint(-42);
        let data = enc.into_bytes();
        let mut dec = KiwiDecoder::new(&data);
        assert_eq!(dec.read_signed_varint().unwrap(), -42);
    }

    #[test]
    fn test_signed_varint_zero() {
        let mut enc = KiwiEncoder::new();
        enc.write_signed_varint(0);
        let data = enc.into_bytes();
        let mut dec = KiwiDecoder::new(&data);
        assert_eq!(dec.read_signed_varint().unwrap(), 0);
    }

    #[test]
    fn test_f32_roundtrip() {
        let mut enc = KiwiEncoder::new();
        enc.write_f32(3.14);
        let data = enc.into_bytes();
        let mut dec = KiwiDecoder::new(&data);
        let v = dec.read_f32().unwrap();
        assert!((v - 3.14).abs() < 0.001);
    }

    #[test]
    fn test_string_roundtrip() {
        let mut enc = KiwiEncoder::new();
        enc.write_string("hello world");
        let data = enc.into_bytes();
        let mut dec = KiwiDecoder::new(&data);
        assert_eq!(dec.read_string().unwrap(), "hello world");
    }

    #[test]
    fn test_string_empty() {
        let mut enc = KiwiEncoder::new();
        enc.write_string("");
        let data = enc.into_bytes();
        let mut dec = KiwiDecoder::new(&data);
        assert_eq!(dec.read_string().unwrap(), "");
    }

    #[test]
    fn test_string_unicode() {
        let mut enc = KiwiEncoder::new();
        enc.write_string("日本語テスト 🎨");
        let data = enc.into_bytes();
        let mut dec = KiwiDecoder::new(&data);
        assert_eq!(dec.read_string().unwrap(), "日本語テスト 🎨");
    }

    #[test]
    fn test_bytes_roundtrip() {
        let mut enc = KiwiEncoder::new();
        enc.write_bytes_value(&[0xDE, 0xAD, 0xBE, 0xEF]);
        let data = enc.into_bytes();
        let mut dec = KiwiDecoder::new(&data);
        assert_eq!(dec.read_bytes().unwrap(), vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn test_bool_true() {
        let mut enc = KiwiEncoder::new();
        enc.write_bool(true);
        let data = enc.into_bytes();
        let mut dec = KiwiDecoder::new(&data);
        assert!(dec.read_bool().unwrap());
    }

    #[test]
    fn test_bool_false() {
        let mut enc = KiwiEncoder::new();
        enc.write_bool(false);
        let data = enc.into_bytes();
        let mut dec = KiwiDecoder::new(&data);
        assert!(!dec.read_bool().unwrap());
    }

    #[test]
    fn test_message_roundtrip() {
        let mut enc = KiwiEncoder::new();
        // Field 1: string "Rectangle 1"
        enc.write_field(1, &KiwiValue::String("Rectangle 1".into()));
        // Field 2: float 100.0
        enc.write_field(2, &KiwiValue::Float(100.0));
        // Field 3: bool true
        enc.write_field(3, &KiwiValue::Bool(true));
        // Terminator
        enc.write_terminator();

        let data = enc.into_bytes();
        let mut dec = KiwiDecoder::new(&data);
        let fields = dec.read_message().unwrap();

        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0].id, 1);
        assert_eq!(fields[0].value.as_str().unwrap(), "Rectangle 1");
        assert_eq!(fields[1].id, 2);
        assert!((fields[1].value.as_float().unwrap() - 100.0).abs() < 0.001);
        assert_eq!(fields[2].id, 3);
        assert!(fields[2].value.as_bool().unwrap());
    }

    #[test]
    fn test_nested_message() {
        let mut enc = KiwiEncoder::new();
        // Field 1: string "Parent"
        enc.write_field(1, &KiwiValue::String("Parent".into()));
        // Field 2: nested message
        enc.write_field(
            2,
            &KiwiValue::Nested(vec![
                KiwiField {
                    id: 1,
                    value: KiwiValue::String("Child".into()),
                },
                KiwiField {
                    id: 2,
                    value: KiwiValue::Float(50.0),
                },
            ]),
        );
        enc.write_terminator();

        let data = enc.into_bytes();
        let mut dec = KiwiDecoder::new(&data);
        let fields = dec.read_message().unwrap();

        assert_eq!(fields.len(), 2);
        let nested = fields[1].value.as_nested().unwrap();
        assert_eq!(nested.len(), 2);
        assert_eq!(nested[0].value.as_str().unwrap(), "Child");
    }

    #[test]
    fn test_array_of_floats() {
        let mut enc = KiwiEncoder::new();
        enc.write_field(
            1,
            &KiwiValue::Array(vec![
                KiwiValue::Float(1.0),
                KiwiValue::Float(2.0),
                KiwiValue::Float(3.0),
            ]),
        );
        enc.write_terminator();

        let data = enc.into_bytes();
        let mut dec = KiwiDecoder::new(&data);
        let fields = dec.read_message().unwrap();

        let arr = fields[0].value.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert!((arr[0].as_float().unwrap() - 1.0).abs() < 0.001);
        assert!((arr[2].as_float().unwrap() - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_empty_array() {
        let mut enc = KiwiEncoder::new();
        enc.write_field(1, &KiwiValue::Array(vec![]));
        enc.write_terminator();

        let data = enc.into_bytes();
        let mut dec = KiwiDecoder::new(&data);
        let fields = dec.read_message().unwrap();

        let arr = fields[0].value.as_array().unwrap();
        assert_eq!(arr.len(), 0);
    }

    #[test]
    fn test_unexpected_eof() {
        let data = [0x80, 0x80]; // varint with continuation bits but no termination
        let mut dec = KiwiDecoder::new(&data);
        assert!(dec.read_varint().is_err());
    }

    #[test]
    fn test_decoder_position_tracking() {
        let mut enc = KiwiEncoder::new();
        enc.write_varint(10);
        enc.write_f32(1.0);
        let data = enc.into_bytes();

        let mut dec = KiwiDecoder::new(&data);
        assert_eq!(dec.position(), 0);
        assert_eq!(dec.remaining(), data.len());

        dec.read_varint().unwrap();
        assert_eq!(dec.position(), 1);

        dec.read_f32().unwrap();
        assert_eq!(dec.position(), 5);
        assert_eq!(dec.remaining(), 0);
        assert!(dec.is_empty());
    }

    #[test]
    fn test_find_field() {
        let fields = vec![
            KiwiField {
                id: 1,
                value: KiwiValue::String("name".into()),
            },
            KiwiField {
                id: 5,
                value: KiwiValue::Float(42.0),
            },
        ];
        assert_eq!(get_string(&fields, 1), Some("name"));
        assert!((get_float(&fields, 5).unwrap() - 42.0).abs() < 0.001);
        assert!(find_field(&fields, 99).is_none());
    }

    #[test]
    fn test_kiwi_value_accessors() {
        assert!(KiwiValue::Bool(true).as_bool().unwrap());
        assert_eq!(KiwiValue::Int(-5).as_int().unwrap(), -5);
        assert_eq!(KiwiValue::UInt(100).as_uint().unwrap(), 100);
        assert_eq!(KiwiValue::String("hi".into()).as_str().unwrap(), "hi");
        assert_eq!(
            KiwiValue::Bytes(vec![1, 2, 3]).as_bytes().unwrap(),
            &[1, 2, 3]
        );

        // Wrong-type access returns None
        assert!(KiwiValue::Bool(true).as_int().is_none());
        assert!(KiwiValue::Float(1.0).as_str().is_none());
        assert!(KiwiValue::String("hi".into()).as_float().is_none());
    }
}
