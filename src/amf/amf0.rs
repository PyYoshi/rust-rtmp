use std::{io, time};

use byteorder::{BigEndian, ReadBytesExt};

use super::Pair;
use super::DecodeResult;
use super::DecodeError;

#[allow(non_snake_case)]
mod Marker {
    pub const NUMBER: u8 = 0x00;
    pub const BOOLEAN: u8 = 0x01;
    pub const STRING: u8 = 0x02;
    pub const OBJECT: u8 = 0x03;
    pub const MOVIECLIP: u8 = 0x04; // reserved
    pub const NULL: u8 = 0x05;
    pub const UNDEFINED: u8 = 0x06;
    pub const REFERENCE: u8 = 0x07; // not supported
    pub const ECMA_ARRAY: u8 = 0x08;
    pub const OBJECT_END: u8 = 0x09;
    pub const STRICT_ARRAY: u8 = 0x0A;
    pub const DATE: u8 = 0x0B;
    pub const LONG_STRING: u8 = 0x0C;
    pub const UNSUPPORTED: u8 = 0x0D;
    pub const RECORDSET: u8 = 0x0E; // reserved
    pub const XML_DOC: u8 = 0x0F;
    pub const TYPED_OBJECT: u8 = 0x10; // not supported
    pub const AVMPLUS: u8 = 0x11;
}


#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Value {
    Number(f64),
    Boolean(bool),
    String(String),
    Object {
        name: Option<String>,
        pairs: Vec<Pair<String, Value>>,
    },
    Null,
    Undefined,
    EcmaArray { pairs: Vec<Pair<String, Value>> },
    Array { pairs: Vec<Value> },
    Date { unixtime: time::Duration },
    LongString(String),
    XmlDoc(String),
}

fn decode_number<R: io::Read>(reader: &mut R) -> DecodeResult<Value> {
    let number = try!(reader.read_f64::<BigEndian>());
    Ok(Value::Number(number))
}

fn decode_boolean<R: io::Read>(reader: &mut R) -> DecodeResult<Value> {
    let boolean = try!(reader.read_u8()) != 0;
    Ok(Value::Boolean(boolean))
}

fn read_utf8<R: io::Read>(reader: &mut R, len: usize) -> DecodeResult<String> {
    let mut b = vec![0; len];
    try!(reader.read_exact(&mut b));
    let u = try!(String::from_utf8(b));
    Ok(u)
}

fn decode_string<R: io::Read>(reader: &mut R) -> DecodeResult<Value> {
    let len = try!(reader.read_u16::<BigEndian>()) as usize;
    read_utf8(reader, len).map(Value::String)
}

fn decode_long_string<R: io::Read>(reader: &mut R) -> DecodeResult<Value> {
    let len = try!(reader.read_u32::<BigEndian>()) as usize;
    read_utf8(reader, len).map(Value::LongString)
}

fn decode_xml_doc<R: io::Read>(reader: &mut R) -> DecodeResult<Value> {
    let len = try!(reader.read_u32::<BigEndian>()) as usize;
    read_utf8(reader, len).map(Value::XmlDoc)
}

fn decode_date<R: io::Read>(reader: &mut R) -> DecodeResult<Value> {
    let ms = try!(reader.read_f64::<BigEndian>());
    try!(reader.read_i16::<BigEndian>()); // skip timezone
    Ok(Value::Date {
        unixtime: time::Duration::from_millis(ms as u64),
    })
}

fn decode_pairs<R: io::Read>(reader: &mut R) -> DecodeResult<Vec<Pair<String, Value>>> {
    let mut v = Vec::new();
    loop {
        let len = try!(reader.read_u16::<BigEndian>()) as usize;
        let key = try!(read_utf8(reader, len));
        match decode_value(reader) {
            Ok(val) => {
                v.push(Pair {
                    key: key,
                    value: val,
                });
            }
            Err(DecodeError::NotExpectedObjectEnd) if key.is_empty() => break,
            Err(e) => return Err(e),
        }
    }
    Ok(v)
}

fn decode_object<R: io::Read>(reader: &mut R) -> DecodeResult<Value> {
    let pairs = try!(decode_pairs(reader));
    Ok(Value::Object {
        name: None,
        pairs: pairs,
    })
}

fn decode_ecma_array<R: io::Read>(reader: &mut R) -> DecodeResult<Value> {
    try!(reader.read_u32::<BigEndian>()) as usize; // skip count
    let pairs = try!(decode_pairs(reader));
    Ok(Value::EcmaArray { pairs: pairs })
}

fn decode_strict_array<R: io::Read>(reader: &mut R) -> DecodeResult<Value> {
    let c = try!(reader.read_u32::<BigEndian>()) as usize;
    let pairs = try!((0..c).map(|_| decode_value(reader)).collect());
    Ok(Value::Array { pairs: pairs })
}

fn decode_typed_object<R: io::Read>(reader: &mut R) -> DecodeResult<Value> {
    let len = try!(reader.read_u16::<BigEndian>()) as usize;
    let name = try!(read_utf8(reader, len));
    let pairs = try!(decode_pairs(reader));
    Ok(Value::Object {
        name: Some(name),
        pairs: pairs,
    })
}

fn decode_value<R: io::Read>(reader: &mut R) -> DecodeResult<Value> {
    let marker = try!(reader.read_u8());
    match marker {
        Marker::NUMBER => decode_number(reader),
        Marker::BOOLEAN => decode_boolean(reader),
        Marker::STRING => decode_string(reader),
        Marker::OBJECT => decode_object(reader),
        Marker::ECMA_ARRAY => decode_ecma_array(reader),
        Marker::STRICT_ARRAY => decode_strict_array(reader),
        Marker::DATE => decode_date(reader),
        Marker::LONG_STRING => decode_long_string(reader),
        Marker::XML_DOC => decode_xml_doc(reader),
        Marker::TYPED_OBJECT => decode_typed_object(reader),

        Marker::OBJECT_END => Err(DecodeError::NotExpectedObjectEnd),

        Marker::NULL => Ok(Value::Null),
        Marker::UNDEFINED => Ok(Value::Undefined),
        Marker::REFERENCE => Err(DecodeError::NotSupportedType { marker }),
        Marker::UNSUPPORTED => Err(DecodeError::NotSupportedType { marker }),
        Marker::RECORDSET => Err(DecodeError::NotSupportedType { marker }),
        Marker::MOVIECLIP => Err(DecodeError::NotSupportedType { marker }),
        Marker::AVMPLUS => Err(DecodeError::NotSupportedType { marker }),

        _ => Err(DecodeError::UnknownType { marker }),
    }
}

pub fn decode<R: io::Read>(reader: &mut R) -> DecodeResult<Value> {
    decode_value(reader)
}

#[cfg(test)]
mod test {
    use std::fs;
    use std::io::BufReader;
    use std::f64;
    use std::time;

    use super::Value;
    use super::decode;
    use super::Pair;
    use super::DecodeError;

    macro_rules! macro_decode {
        ($sample_file: expr) => {
            {
                let mut reader = BufReader::new(fs::File::open(concat!("./testdata/", $sample_file)).unwrap());
                decode(&mut reader)
            }
        }
    }

    macro_rules! macro_decode_equal {
        ($sample_file: expr, $expected: expr) => {
            {
                let value = macro_decode!($sample_file).unwrap();
                assert_eq!(value, $expected)
            }
        }
    }

    #[test]
    fn decode_number() {
        macro_decode_equal!("amf0-number.bin", Value::Number(1234.5));
        macro_decode_equal!(
            "amf0-number-negative-infinity.bin",
            Value::Number(f64::NEG_INFINITY)
        );
        macro_decode_equal!(
            "amf0-number-positive-infinity.bin",
            Value::Number(f64::INFINITY)
        );

        let result_nan = macro_decode!("amf0-number-nan.bin").unwrap();
        if let Value::Number(n) = result_nan {
            assert!(n.is_nan());
        } else {
            assert!(false);
        }
    }

    #[test]
    fn decode_boolean() {
        macro_decode_equal!("amf0-boolean-false.bin", Value::Boolean(false));
        macro_decode_equal!("amf0-boolean-true.bin", Value::Boolean(true));
    }

    #[test]
    fn decode_string() {
        macro_decode_equal!(
            "amf0-string.bin",
            Value::String("Hello, world!".to_string())
        );
    }

    #[test]
    fn decode_long_string() {
        let gen = "うひょおおおおおおおおおおおおおおおおおおおおおおおおおおおおおお".to_string().repeat(2000);
        macro_decode_equal!("amf0-long-string.bin", Value::LongString(gen));
    }

    #[test]
    fn decode_xml_doc() {
        macro_decode_equal!(
            "amf0-xml-doc.bin",
            Value::XmlDoc("<a><b>hello world</b></a>".to_string())
        );
    }

    #[test]
    fn decode_date() {
        macro_decode_equal!(
            "amf0-date.bin",
            Value::Date { unixtime: time::Duration::from_millis(1111111111_000) }
        );
    }

    #[test]
    fn decode_object() {
        let expected = Value::Object {
            name: None,
            pairs: vec![
                Pair {
                    key: "msg".to_string(),
                    value: Value::String("Hello, world! こんにちは、世界！".to_string()),
                },
                Pair {
                    key: "index".to_string(),
                    value: Value::Number(0_f64),
                },
            ],
        };

        macro_decode_equal!("amf0-object.bin", expected);
    }

    #[test]
    fn decode_ecma_array() {
        let expected = Value::EcmaArray {
            pairs: vec![
                Pair {
                    key: "en".to_string(),
                    value: Value::String("Hello, world!".to_string()),
                },
                Pair {
                    key: "ja".to_string(),
                    value: Value::String("こんにちは、世界！".to_string()),
                },
                Pair {
                    key: "zh".to_string(),
                    value: Value::String("你好世界".to_string()),
                },
            ],
        };

        macro_decode_equal!("amf0-ecma-array.bin", expected);
    }

    #[test]
    fn decode_strict_array() {
        let expected = Value::Array {
            pairs: vec![
                Value::Number(1.1),
                Value::Number(2_f64),
                Value::Number(3.3),
                Value::String("こんにちは、世界！".to_string()),
            ],
        };

        macro_decode_equal!("amf0-strict-array.bin", expected);
    }

    #[test]
    fn decode_typed_object() {
        let expected = Value::Object {
            name: Some("com.pyyoshi.hogeclass".to_string()),
            pairs: vec![
                Pair {
                    key: "index".to_string(),
                    value: Value::Number(0_f64),
                },
                Pair {
                    key: "msg".to_string(),
                    value: Value::String("fugaaaaaaa".to_string()),
                },
            ],
        };

        macro_decode_equal!("amf0-typed-object.bin", expected);
    }

    #[test]
    fn decode_null() {
        macro_decode_equal!("amf0-null.bin", Value::Null);
    }

    #[test]
    fn decode_undefined() {
        macro_decode_equal!("amf0-undefined.bin", Value::Undefined);
    }

    #[test]
    fn decode_reference() {
        let expected1 = Value::Array {
            pairs: vec![
                Value::Number(1_f64),
                Value::Number(2_f64),
                Value::Number(3_f64),
            ],
        };
        macro_decode_equal!("amf0-reference-array-number.bin", expected1);

        let expected2 = Value::Array {
            pairs: vec![
                Value::String("foo".to_string()),
                Value::String("baz".to_string()),
                Value::String("bar".to_string()),
            ],
        };
        macro_decode_equal!("amf0-reference-array-string.bin", expected2);

        let expected3 = Value::Object {
            name: None,
            pairs: vec![
                Pair {
                    key: "msg".to_string(),
                    value: Value::String("Hello, world! こんにちは、世界！".to_string()),
                },
                Pair {
                    key: "index".to_string(),
                    value: Value::Number(0_f64),
                },
            ],
        };
        macro_decode_equal!("amf0-reference-object.bin", expected3);
    }

    #[test]
    fn decode_unsupported() {
        assert_eq!(
            macro_decode!("amf0-unsupported.bin"),
            Err(DecodeError::NotSupportedType { marker: 13 })
        );
    }

    #[test]
    fn decode_recordset() {
        assert_eq!(
            macro_decode!("amf0-recordset.bin"),
            Err(DecodeError::NotSupportedType { marker: 14 })
        );
    }

    #[test]
    fn decode_movieclip() {
        assert_eq!(
            macro_decode!("amf0-movieclip.bin"),
            Err(DecodeError::NotSupportedType { marker: 4 })
        );
    }

    #[test]
    fn decode_avmplus() {
        assert_eq!(
            macro_decode!("amf0-avmplus.bin"),
            Err(DecodeError::NotSupportedType { marker: 17 })
        );
    }

    #[test]
    fn decode_object_end() {
        assert_eq!(
            macro_decode!("amf0-object-end.bin"),
            Err(DecodeError::NotExpectedObjectEnd)
        );
    }
}