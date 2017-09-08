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

#[derive(Debug)]
pub struct Decoder<R> {
    reader: R,
    objects: Vec<Value>,
}

impl<R> Decoder<R>
where
    R: io::Read,
{
    pub fn new(reader: R) -> Self {
        Decoder {
            reader: reader,
            objects: Vec::new(),
        }
    }

    pub fn decode(&mut self) -> DecodeResult<Value> {
        self.objects.clear();
        self.decode_value()
    }

    fn read_utf8(&mut self, len: usize) -> DecodeResult<String> {
        let mut b = vec![0; len];
        try!(self.reader.read_exact(&mut b));
        let u = try!(String::from_utf8(b));
        Ok(u)
    }

    fn decode_pairs(&mut self) -> DecodeResult<Vec<Pair<String, Value>>> {
        let mut v = Vec::new();
        loop {
            let len = try!(self.reader.read_u16::<BigEndian>()) as usize;
            let key = try!(self.read_utf8(len));
            match self.decode_value() {
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

    fn decode_number(&mut self) -> DecodeResult<Value> {
        let number = try!(self.reader.read_f64::<BigEndian>());
        Ok(Value::Number(number))
    }

    fn decode_boolean(&mut self) -> DecodeResult<Value> {
        let boolean = try!(self.reader.read_u8()) != 0;
        Ok(Value::Boolean(boolean))
    }

    fn decode_string(&mut self) -> DecodeResult<Value> {
        let len = try!(self.reader.read_u16::<BigEndian>()) as usize;
        self.read_utf8(len).map(Value::String)
    }

    fn decode_long_string(&mut self) -> DecodeResult<Value> {
        let len = try!(self.reader.read_u32::<BigEndian>()) as usize;
        self.read_utf8(len).map(Value::LongString)
    }

    fn decode_xml_doc(&mut self) -> DecodeResult<Value> {
        let len = try!(self.reader.read_u32::<BigEndian>()) as usize;
        self.read_utf8(len).map(Value::XmlDoc)
    }

    fn decode_date(&mut self) -> DecodeResult<Value> {
        let ms = try!(self.reader.read_f64::<BigEndian>());
        try!(self.reader.read_i16::<BigEndian>()); // skip timezone
        Ok(Value::Date {
            unixtime: time::Duration::from_millis(ms as u64),
        })
    }

    fn decode_object(&mut self) -> DecodeResult<Value> {
        let pairs = try!(self.decode_pairs());
        let value = Value::Object {
            name: None,
            pairs: pairs,
        };

        let index = self.objects.len();
        self.objects.push(Value::Null); // 空の値を入れておく
        self.objects[index] = value.clone(); // 空の値を置いたindexに上書き

        Ok(value)
    }

    fn decode_ecma_array(&mut self) -> DecodeResult<Value> {
        try!(self.reader.read_u32::<BigEndian>()) as usize; // skip count
        let pairs = try!(self.decode_pairs());
        let value = Value::EcmaArray { pairs: pairs };

        let index = self.objects.len();
        self.objects.push(Value::Null); // 空の値を入れておく
        self.objects[index] = value.clone(); // 空の値を置いたindexに上書き

        Ok(value)
    }

    fn decode_strict_array(&mut self) -> DecodeResult<Value> {
        let c = try!(self.reader.read_u32::<BigEndian>()) as usize;
        let pairs = try!((0..c).map(|_| self.decode_value()).collect());
        let value = Value::Array { pairs: pairs };

        let index = self.objects.len();
        self.objects.push(Value::Null); // 空の値を入れておく
        self.objects[index] = value.clone(); // 空の値を置いたindexに上書き

        Ok(value)
    }

    fn decode_typed_object(&mut self) -> DecodeResult<Value> {
        let len = try!(self.reader.read_u16::<BigEndian>()) as usize;
        let name = try!(self.read_utf8(len));
        let pairs = try!(self.decode_pairs());
        let value = Value::Object {
            name: Some(name),
            pairs: pairs,
        };

        let index = self.objects.len();
        self.objects.push(Value::Null); // 空の値を入れておく
        self.objects[index] = value.clone(); // 空の値を置いたindexに上書き

        Ok(value)
    }

    // object, typed object, strict array or ecma array
    fn decode_reference(&mut self) -> DecodeResult<Value> {
        let index = try!(self.reader.read_u16::<BigEndian>()) as usize;
        self.objects
            .get(index)
            .ok_or(DecodeError::NotFoundInReferenceTable { index: index })
            .and_then(|v| Ok(v.clone()))
    }

    fn decode_value(&mut self) -> DecodeResult<Value> {
        let marker = try!(self.reader.read_u8());
        match marker {
            Marker::NUMBER => self.decode_number(),
            Marker::BOOLEAN => self.decode_boolean(),
            Marker::STRING => self.decode_string(),
            Marker::OBJECT => self.decode_object(),
            Marker::ECMA_ARRAY => self.decode_ecma_array(),
            Marker::STRICT_ARRAY => self.decode_strict_array(),
            Marker::DATE => self.decode_date(),
            Marker::LONG_STRING => self.decode_long_string(),
            Marker::XML_DOC => self.decode_xml_doc(),
            Marker::TYPED_OBJECT => self.decode_typed_object(),
            Marker::REFERENCE => self.decode_reference(),
            Marker::NULL => Ok(Value::Null),
            Marker::UNDEFINED => Ok(Value::Undefined),

            Marker::OBJECT_END => Err(DecodeError::NotExpectedObjectEnd),
            Marker::UNSUPPORTED => Err(DecodeError::NotSupportedType { marker }),
            Marker::RECORDSET => Err(DecodeError::NotSupportedType { marker }),
            Marker::MOVIECLIP => Err(DecodeError::NotSupportedType { marker }),
            Marker::AVMPLUS => Err(DecodeError::NotSupportedType { marker }),

            _ => Err(DecodeError::UnknownType { marker }),
        }
    }
}

#[cfg(test)]
mod test {
    use std::fs;
    use std::io::BufReader;
    use std::f64;
    use std::time;

    use super::Value;
    use super::Pair;
    use super::DecodeError;
    use super::Decoder;

    macro_rules! macro_decode {
        ($sample_file: expr) => {
            {
                let mut decoder = Decoder::new(
                    BufReader::new(fs::File::open(
                        concat!("./testdata/", $sample_file)).unwrap()
                    )
                );
                decoder.decode()
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
