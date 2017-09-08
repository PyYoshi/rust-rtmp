use std::{io, time};

use byteorder::{BigEndian, ReadBytesExt};

use super::{Pair, DecodeResult, DecodeError};

#[allow(non_snake_case)]
pub mod Marker {
    pub const UNDEFINED: u8 = 0x00;
    pub const NULL: u8 = 0x01;
    pub const FALSE: u8 = 0x02;
    pub const TRUE: u8 = 0x03;
    pub const INTEGER: u8 = 0x04;
    pub const DOUBLE: u8 = 0x05;
    pub const STRING: u8 = 0x06;
    pub const XML_DOC: u8 = 0x07;
    pub const DATE: u8 = 0x08;
    pub const ARRAY: u8 = 0x09; // not supported
    pub const OBJECT: u8 = 0x0A;
    pub const XML: u8 = 0x0B;
    pub const BYTE_ARRAY: u8 = 0x0C; // not supported
    pub const VECTOR_INT: u8 = 0x0D; // not supported
    pub const VECTOR_UINT: u8 = 0x0E; // not supported
    pub const VECTOR_DOUBLE: u8 = 0x0F; // not supported
    pub const VECTOR_OBJECT: u8 = 0x10; // not supported
    pub const DICTIONARY: u8 = 0x11; // not supported
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Value {
    Undefined,
    Null,
    Boolean(bool),
    Integer(i32),
    Double(f64),
    String(String),
    XmlDoc(String),
    Date { unixtime: time::Duration },
    Object {
        name: Option<String>,
        pairs: Vec<Pair<String, Value>>,
    },
    Xml(String),
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

    // 1.3.1 Variable Length Unsigned 29-bit Integer Encoding
    // AMF 3 makes use of a special compact format for writing integers to reduce the number
    // of bytes required for encoding. As with a normal 32-bit integer, up to 4 bytes are required
    // to hold the value however the high bit of the first 3 bytes are used as flags to determine
    // whether the next byte is part of the integer. With up to 3 bits of the 32 bits being used as
    // flags, only 29 significant bits remain for encoding an integer. This means the largest
    // unsigned integer value that can be represented is 229 - 1.
    // (hex) : (binary)
    // 0x00000000 - 0x0000007F : 0xxxxxxx
    // 0x00000080 - 0x00003FFF : 1xxxxxxx 0xxxxxxx
    // 0x00004000 - 0x001FFFFF : 1xxxxxxx 1xxxxxxx 0xxxxxxx
    // 0x00200000 - 0x3FFFFFFF : 1xxxxxxx 1xxxxxxx 1xxxxxxx xxxxxxxx
    // 0x40000000 - 0xFFFFFFFF : throw range exception
    // In ABNF syntax, the variable length unsigned 29-bit integer type is described as follows:
    // U29 = U29-1 | U29-2 | U29-3 | U29-4
    // U29-1 = %x00-7F
    // U29-2 = %x80-FF %x00-7F
    // U29-3 = %x80-FF %x80-FF %x00-7F
    // U29-4 = %x80-FF %x80-FF %x80-FF %x00-FF
    fn decode_u29(&mut self) -> DecodeResult<u32> {
        let mut n = 0;
        for _ in 0..3 {
            let b = try!(self.reader.read_u8()) as u32;
            n = (n << 7) | (b & 0x7f);
            if (b & 0x80) == 0 {
                return Ok(n);
            }
        }
        let b = try!(self.reader.read_u8()) as u32;
        n = (n << 8) | b;
        Ok(n)
    }

    fn read_bytes(&mut self, len: usize) -> DecodeResult<Vec<u8>> {
        let mut buf = vec![0; len];
        try!(self.reader.read_exact(&mut buf));
        Ok(buf)
    }

    fn decode_utf8(&mut self) -> DecodeResult<String> {
        let u29 = try!(self.decode_u29()) as usize;
        let is_reference = (u29 & 0x1) == 0;
        let value = u29 >> 1;
        if is_reference {
            Err(DecodeError::NotSupportedReferenceTables { index: value })
        } else {
            let bytes = try!(self.read_bytes(value));
            let s = try!(String::from_utf8(bytes));
            Ok(s)
        }
    }

    fn decode_pairs(&mut self) -> DecodeResult<Vec<Pair<String, Value>>> {
        let mut pairs = Vec::new();
        loop {
            let key = try!(self.decode_utf8());
            if key.is_empty() {
                return Ok(pairs);
            }
            let value = try!(self.decode_value());
            pairs.push(Pair {
                key: key,
                value: value,
            });
        }
    }

    fn decode_integer(&mut self) -> DecodeResult<Value> {
        let num = try!(self.decode_u29()) as i32;
        let num = if num >= (1 << 28) {
            num - (1 << 29)
        } else {
            num
        };
        Ok(Value::Integer(num))
    }

    fn decode_double(&mut self) -> DecodeResult<Value> {
        let n = try!(self.reader.read_f64::<BigEndian>());
        Ok(Value::Double(n))
    }

    fn decode_string(&mut self) -> DecodeResult<Value> {
        let s = try!(self.decode_utf8());
        Ok(Value::String(s))
    }

    fn decode_xml_doc(&mut self) -> DecodeResult<Value> {
        let s = try!(self.decode_utf8());
        Ok(Value::XmlDoc(s))
    }
    fn decode_date(&mut self) -> DecodeResult<Value> {
        try!(self.decode_u29()) as usize; // skip
        let millis = try!(self.reader.read_f64::<BigEndian>());
        Ok(Value::Date {
            unixtime: time::Duration::from_millis(millis as u64),
        })
    }

    fn decode_object(&mut self) -> DecodeResult<Value> {
        let u29 = try!(self.decode_u29()) as usize;
        let is_reference = (u29 & 0x1) == 0;
        let value = u29 >> 1;
        if is_reference {
            Err(DecodeError::NotSupportedReferenceTables { index: value })
        } else {
            if (value & 0x1) == 0 {
                Err(DecodeError::NotSupportedReferenceTables { index: value })
            } else {
                let class_ref = value >> 0x1;
                if (class_ref & 0x1) == 0 {
                    let class_index = class_ref >> 0x1;
                    Err(DecodeError::NotSupportedReferenceTables {
                        index: class_index,
                    })
                } else {
                    let class_ext_ref = class_ref >> 0x1;

                    let is_externalizable = (class_ext_ref & 0x1) == 1;
                    let is_dynamic = ((class_ext_ref >> 0x1) & 0x1) == 1;

                    let cdnum = class_ext_ref >> 0x2;

                    let class_name = try!(self.decode_utf8());
                    let fields: Vec<String> =
                        try!((0..cdnum).map(|_| self.decode_utf8()).collect());

                    if is_externalizable {
                        let class_name = try!(self.decode_utf8());
                        Err(DecodeError::ExternalizableType { name: class_name })
                    } else {
                        let mut pairs = try!(
                            fields
                                .iter()
                                .map(|k| {
                                    Ok(Pair {
                                        key: k.clone(),
                                        value: try!(self.decode_value()),
                                    })
                                })
                                .collect::<DecodeResult<Vec<_>>>()
                        );

                        if is_dynamic {
                            pairs.extend(try!(self.decode_pairs()));
                        }

                        Ok(Value::Object {
                            name: Some(class_name),
                            pairs: pairs,
                        })
                    }
                }
            }
        }
    }

    fn decode_xml(&mut self) -> DecodeResult<Value> {
        let s = try!(self.decode_utf8());
        Ok(Value::Xml(s))
    }

    fn decode_value(&mut self) -> DecodeResult<Value> {
        let marker = try!(self.reader.read_u8());
        match marker {
            Marker::UNDEFINED => Ok(Value::Undefined),
            Marker::NULL => Ok(Value::Null),
            Marker::FALSE => Ok(Value::Boolean(false)),
            Marker::TRUE => Ok(Value::Boolean(true)),
            Marker::INTEGER => self.decode_integer(),
            Marker::DOUBLE => self.decode_double(),
            Marker::STRING => self.decode_string(),
            Marker::XML_DOC => self.decode_xml_doc(),
            Marker::DATE => self.decode_date(),
            Marker::OBJECT => self.decode_object(),
            Marker::XML => self.decode_xml(),

            Marker::ARRAY => Err(DecodeError::NotSupportedType { marker }),
            Marker::BYTE_ARRAY => Err(DecodeError::NotSupportedType { marker }),
            Marker::VECTOR_INT => Err(DecodeError::NotSupportedType { marker }),
            Marker::VECTOR_UINT => Err(DecodeError::NotSupportedType { marker }),
            Marker::VECTOR_DOUBLE => Err(DecodeError::NotSupportedType { marker }),
            Marker::VECTOR_OBJECT => Err(DecodeError::NotSupportedType { marker }),
            Marker::DICTIONARY => Err(DecodeError::NotSupportedType { marker }),

            _ => Err(DecodeError::UnknownType { marker }),
        }
    }
}

#[cfg(test)]
mod test {
    use std::fs;
    use std::io::BufReader;
    use std::time;

    use super::Value;
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
    fn decode_integer() {
        macro_decode_equal!("amf3-integer-0.bin", Value::Integer(0));
        macro_decode_equal!("amf3-integer-128.bin", Value::Integer(128));
        macro_decode_equal!("amf3-integer-16384.bin", Value::Integer(16384));
        macro_decode_equal!("amf3-integer-max-u29.bin", Value::Integer(268435455));
        macro_decode_equal!("amf3-integer-min-u29.bin", Value::Integer(-268435456));
    }

    #[test]
    fn decode_double() {
        macro_decode_equal!("amf3-double-max-u29.bin", Value::Double(268435456_f64));
        macro_decode_equal!("amf3-double-min-u29.bin", Value::Double(-268435457_f64));
        macro_decode_equal!("amf3-double-pi.bin", Value::Double(3.14_f64));
    }

    #[test]
    fn decode_string() {
        macro_decode_equal!(
            "amf3-string.bin",
            Value::String("こんにちは、世界！".to_string())
        );
    }

    #[test]
    fn decode_xml_doc() {
        macro_decode_equal!(
            "amf3-xml-doc.bin",
            Value::XmlDoc("<a><b>hello world</b></a>".to_string())
        );
    }

    #[test]
    fn decode_date() {
        macro_decode_equal!(
            "amf3-date.bin",
            Value::Date { unixtime: time::Duration::from_millis(1111111111_000) }
        );
    }

    #[test]
    fn decode_object() {
        assert!(false, "Not implemented");

        let value = macro_decode!("amf3-object.bin");
        println!("{:?}", value);
    }

    #[test]
    fn decode_xml() {
        macro_decode_equal!(
            "amf3-xml.bin",
            Value::Xml("<a><b>hello world</b></a>".to_string())
        );
    }

    #[test]
    fn decode_undefined() {
        macro_decode_equal!("amf3-undefined.bin", Value::Undefined);
    }

    #[test]
    fn decode_null() {
        macro_decode_equal!("amf3-null.bin", Value::Null);
    }

    #[test]
    fn decode_boolean() {
        macro_decode_equal!("amf3-boolean-false.bin", Value::Boolean(false));
        macro_decode_equal!("amf3-boolean-true.bin", Value::Boolean(true));
    }

    #[test]
    fn decode_array() {
        assert!(false, "Not implemented");

        let value = macro_decode!("amf3-array.bin");
        println!("{:?}", value);
    }

    #[test]
    fn decode_byte_array() {
        assert!(false, "Not implemented");

        let value = macro_decode!("amf3-byte-array-object.bin");
        println!("{:?}", value);

        let value = macro_decode!("amf3-byte-array.bin");
        println!("{:?}", value);
    }

    #[test]
    fn decode_vector_int() {
        assert!(false, "Not implemented");

        let value = macro_decode!("amf3-vector-int.bin");
        println!("{:?}", value);
    }

    #[test]
    fn decode_vector_uint() {
        assert!(false, "Not implemented");

        let value = macro_decode!("amf3-vector-uint.bin");
        println!("{:?}", value);
    }

    #[test]
    fn decode_vector_double() {
        assert!(false, "Not implemented");

        let value = macro_decode!("amf3-vector-double.bin");
        println!("{:?}", value);
    }

    #[test]
    fn decode_vector_object() {
        assert!(false, "Not implemented");

        let value = macro_decode!("amf3-vector-object.bin");
        println!("{:?}", value);
    }

    #[test]
    fn decode_dictionary() {
        assert!(false, "Not implemented");

        let value = macro_decode!("amf3-dictionary.bin");
        println!("{:?}", value);
    }
}
