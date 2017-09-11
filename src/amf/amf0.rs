use std::{io, time};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use super::{Pair, DecodeResult, EncodeResult, DecodeError};
use super::amf3;

#[allow(non_snake_case)]
mod Marker {
    pub const NUMBER: u8 = 0x00;
    pub const BOOLEAN: u8 = 0x01;
    pub const STRING: u8 = 0x02;
    pub const OBJECT: u8 = 0x03;
    pub const MOVIECLIP: u8 = 0x04; // reserved
    pub const NULL: u8 = 0x05;
    pub const UNDEFINED: u8 = 0x06;
    pub const REFERENCE: u8 = 0x07;
    pub const ECMA_ARRAY: u8 = 0x08;
    pub const OBJECT_END: u8 = 0x09;
    pub const STRICT_ARRAY: u8 = 0x0A;
    pub const DATE: u8 = 0x0B;
    pub const LONG_STRING: u8 = 0x0C;
    pub const UNSUPPORTED: u8 = 0x0D;
    pub const RECORDSET: u8 = 0x0E; // reserved
    pub const XML_DOC: u8 = 0x0F;
    pub const TYPED_OBJECT: u8 = 0x10;
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
    Array { values: Vec<Value> },
    Date { unixtime: time::Duration },
    LongString(String),
    XmlDoc(String),
    AvmPlus(amf3::Value),
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
        let value = Value::Array { values: pairs };

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

    fn decode_avmplus(&mut self) -> DecodeResult<Value> {
        let value = try!(amf3::Decoder::new(&mut self.reader).decode());
        Ok(Value::AvmPlus(value))
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
            Marker::AVMPLUS => self.decode_avmplus(),
            Marker::NULL => Ok(Value::Null),
            Marker::UNDEFINED => Ok(Value::Undefined),

            Marker::OBJECT_END => Err(DecodeError::NotExpectedObjectEnd),
            Marker::UNSUPPORTED => Err(DecodeError::NotSupportedType { marker }),
            Marker::RECORDSET => Err(DecodeError::NotSupportedType { marker }),
            Marker::MOVIECLIP => Err(DecodeError::NotSupportedType { marker }),

            _ => Err(DecodeError::UnknownType { marker }),
        }
    }
}

#[derive(Debug)]
pub struct Encoder<W> {
    writer: W,
}

impl<W> Encoder<W>
where
    W: io::Write,
{
    pub fn new(writer: W) -> Self {
        Encoder { writer: writer }
    }

    pub fn encode(&mut self, value: &Value) -> EncodeResult<()> {
        self.encode_value(value)
    }

    fn write_string(&mut self, s: &str) -> EncodeResult<()> {
        assert!(s.len() <= 0xFFFF);
        try!(self.writer.write_u16::<BigEndian>(s.len() as u16));
        try!(self.writer.write_all(s.as_bytes()));
        Ok(())
    }

    fn write_long_string(&mut self, s: &str) -> EncodeResult<()> {
        assert!(s.len() <= 0xFFFF_FFFF);
        try!(self.writer.write_u32::<BigEndian>(s.len() as u32));
        try!(self.writer.write_all(s.as_bytes()));
        Ok(())
    }

    fn encode_pairs(&mut self, pairs: &[Pair<String, Value>]) -> EncodeResult<()> {
        for pair in pairs {
            try!(self.write_string(&pair.key));
            try!(self.encode_value(&pair.value));
        }
        try!(self.writer.write_u16::<BigEndian>(0)); // UTF-8-empty => u16
        try!(self.writer.write_u8(Marker::OBJECT_END));
        Ok(())
    }

    fn encode_number(&mut self, number: f64) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::NUMBER));
        try!(self.writer.write_f64::<BigEndian>(number));
        Ok(())
    }

    fn encode_boolean(&mut self, boolean: bool) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::BOOLEAN));
        try!(self.writer.write_u8(boolean as u8));
        Ok(())
    }

    fn encode_string(&mut self, string: &str) -> EncodeResult<()> {
        if string.len() <= 0xFFFF {
            try!(self.writer.write_u8(Marker::STRING));
            try!(self.write_string(&string));
        } else {
            try!(self.writer.write_u8(Marker::LONG_STRING));
            try!(self.write_long_string(&string));
        }
        Ok(())
    }

    fn encode_xml_doc(&mut self, xml_doc: &str) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::XML_DOC));
        try!(self.write_long_string(&xml_doc));
        Ok(())
    }

    fn encode_date(&mut self, unixtime: time::Duration) -> EncodeResult<()> {
        let ms = unixtime.as_secs() * 1000 + (unixtime.subsec_nanos() as u64) / 1000_000;
        try!(self.writer.write_u8(Marker::DATE));
        try!(self.writer.write_f64::<BigEndian>(ms as f64));
        try!(self.writer.write_i16::<BigEndian>(0));
        Ok(())
    }

    // TODO: reference tableのサポート
    fn encode_object(
        &mut self,
        name: &Option<String>,
        pairs: &[Pair<String, Value>],
    ) -> EncodeResult<()> {
        if let Some(name) = name.as_ref() {
            try!(self.writer.write_u8(Marker::TYPED_OBJECT));
            try!(self.write_string(name));
        } else {
            try!(self.writer.write_u8(Marker::OBJECT));
        }
        try!(self.encode_pairs(pairs));
        Ok(())
    }

    // TODO: reference tableのサポート
    fn encode_ecma_array(&mut self, pairs: &[Pair<String, Value>]) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::ECMA_ARRAY));
        try!(self.writer.write_u32::<BigEndian>(pairs.len() as u32)); // associative-count => u32
        try!(self.encode_pairs(pairs));
        Ok(())
    }

    // TODO: reference tableのサポート
    fn encode_strict_array(&mut self, values: &[Value]) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::STRICT_ARRAY));
        try!(self.writer.write_u32::<BigEndian>(values.len() as u32)); // array-count => u32
        for v in values {
            try!(self.encode_value(v));
        }
        Ok(())
    }

    fn encode_avmplus(&mut self, value: &amf3::Value) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::AVMPLUS));
        try!(amf3::Encoder::new(&mut self.writer).encode(value));
        Ok(())
    }

    fn encode_null(&mut self) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::NULL));
        Ok(())
    }

    fn encode_undefined(&mut self) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::UNDEFINED));
        Ok(())
    }

    fn encode_value(&mut self, value: &Value) -> EncodeResult<()> {
        match *value {
            Value::Number(number) => self.encode_number(number),
            Value::Boolean(boolean) => self.encode_boolean(boolean),
            Value::String(ref string) => self.encode_string(string),
            Value::Object {
                ref name,
                ref pairs,
            } => self.encode_object(name, pairs),
            Value::EcmaArray { ref pairs } => self.encode_ecma_array(pairs),
            Value::Array { ref values } => self.encode_strict_array(values),
            Value::Date { unixtime } => self.encode_date(unixtime),
            Value::LongString(ref string) => self.encode_string(string),
            Value::XmlDoc(ref xml_doc) => self.encode_xml_doc(xml_doc),
            Value::AvmPlus(ref value) => self.encode_avmplus(value),
            Value::Null => self.encode_null(),
            Value::Undefined => self.encode_undefined(),
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
    use super::amf3;
    use super::Encoder;

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

    macro_rules! macro_encode_equal {
        ($value:expr, $file:expr) => {
            {
                let expected = include_bytes!(concat!("../../testdata/", $file));
                let mut buf = Vec::new();
                let _ = Encoder::new(&mut buf).encode(& $value);
                // println!("==== {:?} ====", $file);
                // println!("value:    {:?}", buf);
                // println!("expected: {:?}", expected);
                assert_eq!(buf, &expected[..])
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
            values: vec![
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
            values: vec![
                Value::Number(1_f64),
                Value::Number(2_f64),
                Value::Number(3_f64),
            ],
        };
        macro_decode_equal!("amf0-reference-array-number.bin", expected1);

        let expected2 = Value::Array {
            values: vec![
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
        let expected = Value::AvmPlus(amf3::Value::Object {
            name: None,
            sealed_count: 2,
            pairs: vec![
                Pair {
                    key: "operation".to_string(),
                    value: amf3::Value::Integer(5),
                },
                Pair {
                    key: "timestamp".to_string(),
                    value: amf3::Value::Integer(0),
                },
            ],
        });
        macro_decode_equal!("amf0-avmplus.bin", expected);
    }

    #[test]
    fn decode_object_end() {
        assert_eq!(
            macro_decode!("amf0-object-end.bin"),
            Err(DecodeError::NotExpectedObjectEnd)
        );
    }

    #[test]
    fn encode_number() {
        macro_encode_equal!(Value::Number(1234.5), "amf0-number.bin");
        macro_encode_equal!(
            Value::Number(f64::NEG_INFINITY),
            "amf0-number-negative-infinity.bin"
        );
        macro_encode_equal!(
            Value::Number(f64::INFINITY),
            "amf0-number-positive-infinity.bin"
        );
    }

    #[test]
    fn encode_boolean() {
        macro_encode_equal!(Value::Boolean(false), "amf0-boolean-false.bin");
        macro_encode_equal!(Value::Boolean(true), "amf0-boolean-true.bin");
    }

    #[test]
    fn encode_string() {
        macro_encode_equal!(
            Value::String("Hello, world!".to_string()),
            "amf0-string.bin"
        );
    }

    #[test]
    fn encode_long_string() {
        let gen = "うひょおおおおおおおおおおおおおおおおおおおおおおおおおおおおおお".to_string().repeat(2000);
        macro_encode_equal!(Value::LongString(gen), "amf0-long-string.bin");
    }

    #[test]
    fn encode_xml_doc() {
        macro_encode_equal!(
            Value::XmlDoc("<a><b>hello world</b></a>".to_string()),
            "amf0-xml-doc.bin"
        );
    }

    #[test]
    fn encode_date() {
        macro_encode_equal!(
            Value::Date { unixtime: time::Duration::from_millis(1111111111_000) },
            "amf0-date.bin"
        );
    }

    #[test]
    fn encode_object() {
        let value = Value::Object {
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
        macro_encode_equal!(value, "amf0-object.bin");
    }

    #[test]
    fn encode_ecma_array() {
        let value = Value::EcmaArray {
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
        macro_encode_equal!(value, "amf0-ecma-array.bin");
    }

    #[test]
    fn encode_strict_array() {
        let value = Value::Array {
            values: vec![
                Value::Number(1.1),
                Value::Number(2_f64),
                Value::Number(3.3),
                Value::String("こんにちは、世界！".to_string()),
            ],
        };
        macro_encode_equal!(value, "amf0-strict-array.bin");
    }

    #[test]
    fn encode_typed_object() {
        let value = Value::Object {
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
        macro_encode_equal!(value, "amf0-typed-object.bin");
    }

    #[test]
    fn encode_null() {
        macro_encode_equal!(Value::Null, "amf0-null.bin");
    }

    #[test]
    fn encode_undefined() {
        macro_encode_equal!(Value::Undefined, "amf0-undefined.bin");
    }

    #[test]
    fn encode_avmplus() {
        let value = Value::AvmPlus(amf3::Value::Array {
            assoc_entries: vec![],
            dense_entries: vec![
                amf3::Value::Double(1.1),
                amf3::Value::Integer(2),
                amf3::Value::Double(3.3),
                amf3::Value::String("こんにちは、世界！".to_string()),
            ],
        });
        macro_encode_equal!(value, "amf0-avmplus-array.bin");
    }
}
