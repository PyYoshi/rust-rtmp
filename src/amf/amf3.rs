use std::{io, time};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use super::{Pair, DecodeResult, DecodeError, EncodeResult, EncodeError};

pub const MAX_29B_INT: i32 = 0x0FFF_FFFF;
pub const MIN_29B_INT: i32 = -0x1000_0000;

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
    pub const ARRAY: u8 = 0x09;
    pub const OBJECT: u8 = 0x0A;
    pub const XML: u8 = 0x0B;
    pub const BYTE_ARRAY: u8 = 0x0C;
    pub const VECTOR_INT: u8 = 0x0D;
    pub const VECTOR_UINT: u8 = 0x0E;
    pub const VECTOR_DOUBLE: u8 = 0x0F;
    pub const VECTOR_OBJECT: u8 = 0x10;
    pub const DICTIONARY: u8 = 0x11;
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
        sealed_count: usize,
        pairs: Vec<Pair<String, Value>>,
    },
    Xml(String),
    Array {
        assoc_entries: Vec<Pair<String, Value>>,
        dense_entries: Vec<Value>,
    },
    ByteArray(Vec<u8>),
    IntVector { is_fixed: bool, entries: Vec<i32> },
    UintVector { is_fixed: bool, entries: Vec<u32> },
    DoubleVector { is_fixed: bool, entries: Vec<f64> },
    ObjectVector {
        name: Option<String>,
        is_fixed: bool,
        entries: Vec<Value>,
    },
    Dictionary {
        is_weak: bool,
        entries: Vec<Pair<Value, Value>>,
    },
}

#[derive(Debug, Clone)]
struct Class {
    name: Option<String>,
    is_dynamic: bool,
    fields: Vec<String>,
}

#[derive(Debug)]
pub struct Decoder<R> {
    reader: R,
    objects: Vec<Value>,
    strings: Vec<String>,
    classes: Vec<Class>,
}

impl<R> Decoder<R>
where
    R: io::Read,
{
    pub fn new(reader: R) -> Self {
        Decoder {
            reader: reader,
            objects: Vec::new(),
            strings: Vec::new(),
            classes: Vec::new(),
        }
    }

    pub fn decode(&mut self) -> DecodeResult<Value> {
        self.objects.clear();
        self.strings.clear();
        self.classes.clear();
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

        if is_reference {
            let index = u29 >> 1;
            self.strings
                .get(index)
                .ok_or(DecodeError::NotFoundInReferenceTable { index: index })
                .and_then(|v| Ok(v.clone()))
        } else {
            let size = u29 >> 1;
            let bytes = try!(self.read_bytes(size));
            let s = try!(String::from_utf8(bytes));
            if !s.is_empty() {
                self.strings.push(s.clone());
            }
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
        let u29 = try!(self.decode_u29()) as usize;
        let is_reference = (u29 & 0x1) == 0;

        if is_reference {
            let index = u29 >> 1;
            self.objects
                .get(index)
                .ok_or(DecodeError::NotFoundInReferenceTable { index: index })
                .and_then(|v| Ok(v.clone()))
        } else {
            let size = u29 >> 1;
            self.read_bytes(size)
                .and_then(|b| Ok(try!(String::from_utf8(b))))
                .map(Value::XmlDoc)
        }
    }

    fn decode_date(&mut self) -> DecodeResult<Value> {
        try!(self.decode_u29()) as usize; // skip
        let millis = try!(self.reader.read_f64::<BigEndian>());
        Ok(Value::Date {
            unixtime: time::Duration::from_millis(millis as u64),
        })
    }

    fn decode_xml(&mut self) -> DecodeResult<Value> {
        let u29 = try!(self.decode_u29()) as usize;
        let is_reference = (u29 & 0x1) == 0;

        if is_reference {
            let index = u29 >> 1;
            self.objects
                .get(index)
                .ok_or(DecodeError::NotFoundInReferenceTable { index: index })
                .and_then(|v| Ok(v.clone()))
        } else {
            let size = u29 >> 1;
            self.read_bytes(size)
                .and_then(|b| Ok(try!(String::from_utf8(b))))
                .map(Value::Xml)
        }
    }

    fn decode_object(&mut self) -> DecodeResult<Value> {
        let u29 = try!(self.decode_u29()) as usize;
        let is_reference = (u29 & 0x1) == 0;

        if is_reference {
            let index = u29 >> 1;
            self.objects
                .get(index)
                .ok_or(DecodeError::NotFoundInReferenceTable { index: index })
                .and_then(|v| Ok(v.clone()))
        } else {
            let size = u29 >> 1;
            if (size & 0x1) == 0 {
                let index = size >> 0x1;
                let klass = try!(self.classes.get(index).ok_or(
                    DecodeError::NotFoundInReferenceTable { index: index },
                )).clone();

                let mut pairs = try!(
                    klass
                        .fields
                        .iter()
                        .map(|k| {
                            Ok(Pair {
                                key: k.clone(),
                                value: try!(self.decode_value()),
                            })
                        })
                        .collect::<DecodeResult<Vec<_>>>()
                );

                if klass.is_dynamic {
                    pairs.extend(try!(self.decode_pairs()));
                }
                Ok(Value::Object {
                    name: klass.name,
                    sealed_count: pairs.len(),
                    pairs: pairs,
                })
            } else if (size & 0b10) != 0 {
                let class_name = try!(self.decode_utf8());
                Err(DecodeError::ExternalizableType { name: class_name })
            } else {
                let is_dynamic = (size & 0b100) != 0;
                let field_num = size >> 3;
                let class_name = try!(self.decode_utf8());
                let fields = try!((0..field_num).map(|_| self.decode_utf8()).collect());

                let klass = Class {
                    name: if class_name.is_empty() {
                        None
                    } else {
                        Some(class_name)
                    },
                    is_dynamic: is_dynamic,
                    fields: fields,
                };
                self.classes.push(klass.clone());
                let mut pairs = try!(
                    klass
                        .fields
                        .iter()
                        .map(|k| {
                            Ok(Pair {
                                key: k.clone(),
                                value: try!(self.decode_value()),
                            })
                        })
                        .collect::<DecodeResult<Vec<_>>>()
                );
                if klass.is_dynamic {
                    pairs.extend(try!(self.decode_pairs()));
                }
                Ok(Value::Object {
                    name: klass.name,
                    sealed_count: pairs.len(),
                    pairs: pairs,
                })
            }
        }
    }

    fn decode_array(&mut self) -> DecodeResult<Value> {
        let u29 = try!(self.decode_u29()) as usize;
        let is_reference = (u29 & 0x01) == 0;

        if is_reference {
            let index = u29 >> 1;
            self.objects
                .get(index)
                .ok_or(DecodeError::NotFoundInReferenceTable { index: index })
                .and_then(|v| Ok(v.clone()))
        } else {
            let index = self.objects.len();
            self.objects.push(Value::Null);

            let size = u29 >> 1;
            let assoc = try!(self.decode_pairs());
            let dense = try!((0..size).map(|_| self.decode_value()).collect());

            let value = Value::Array {
                assoc_entries: assoc,
                dense_entries: dense,
            };

            self.objects[index] = value.clone();
            Ok(value)
        }
    }

    fn decode_byte_array(&mut self) -> DecodeResult<Value> {
        let u29 = try!(self.decode_u29()) as usize;
        let is_reference = (u29 & 0x01) == 0;

        if is_reference {
            let index = u29 >> 1;
            self.objects
                .get(index)
                .ok_or(DecodeError::NotFoundInReferenceTable { index: index })
                .and_then(|v| Ok(v.clone()))
        } else {
            let index = self.objects.len();
            self.objects.push(Value::Null);

            let size = u29 >> 1;
            let value = Value::ByteArray(try!(self.read_bytes(size)));

            self.objects[index] = value.clone();
            Ok(value)
        }
    }

    fn decode_vector_int(&mut self) -> DecodeResult<Value> {
        let u29 = try!(self.decode_u29()) as usize;
        let is_reference = (u29 & 0x01) == 0;

        if is_reference {
            let index = u29 >> 1;
            self.objects
                .get(index)
                .ok_or(DecodeError::NotFoundInReferenceTable { index: index })
                .and_then(|v| Ok(v.clone()))
        } else {
            let index = self.objects.len();
            self.objects.push(Value::Null);

            let size = u29 >> 1;
            let is_fixed = try!(self.reader.read_u8()) != 0;
            let entries = try!(
                (0..size)
                    .map(|_| self.reader.read_i32::<BigEndian>())
                    .collect()
            );

            let value = Value::IntVector {
                is_fixed: is_fixed,
                entries: entries,
            };

            self.objects[index] = value.clone();
            Ok(value)
        }
    }

    fn decode_vector_uint(&mut self) -> DecodeResult<Value> {
        let u29 = try!(self.decode_u29()) as usize;
        let is_reference = (u29 & 0x01) == 0;

        if is_reference {
            let index = u29 >> 1;
            self.objects
                .get(index)
                .ok_or(DecodeError::NotFoundInReferenceTable { index: index })
                .and_then(|v| Ok(v.clone()))
        } else {
            let index = self.objects.len();
            self.objects.push(Value::Null);

            let size = u29 >> 1;
            let is_fixed = try!(self.reader.read_u8()) != 0;
            let entries = try!(
                (0..size)
                    .map(|_| self.reader.read_u32::<BigEndian>())
                    .collect()
            );

            let value = Value::UintVector {
                is_fixed: is_fixed,
                entries: entries,
            };

            self.objects[index] = value.clone();
            Ok(value)
        }
    }

    fn decode_vector_double(&mut self) -> DecodeResult<Value> {
        let u29 = try!(self.decode_u29()) as usize;
        let is_reference = (u29 & 0x01) == 0;

        if is_reference {
            let index = u29 >> 1;
            self.objects
                .get(index)
                .ok_or(DecodeError::NotFoundInReferenceTable { index: index })
                .and_then(|v| Ok(v.clone()))
        } else {
            let index = self.objects.len();
            self.objects.push(Value::Null);

            let size = u29 >> 1;
            let is_fixed = try!(self.reader.read_u8()) != 0;
            let entries = try!(
                (0..size)
                    .map(|_| self.reader.read_f64::<BigEndian>())
                    .collect()
            );

            let value = Value::DoubleVector {
                is_fixed: is_fixed,
                entries: entries,
            };

            self.objects[index] = value.clone();
            Ok(value)
        }
    }

    fn decode_vector_object(&mut self) -> DecodeResult<Value> {
        let u29 = try!(self.decode_u29()) as usize;
        let is_reference = (u29 & 0x01) == 0;

        if is_reference {
            let index = u29 >> 1;
            self.objects
                .get(index)
                .ok_or(DecodeError::NotFoundInReferenceTable { index: index })
                .and_then(|v| Ok(v.clone()))
        } else {
            let index = self.objects.len();
            self.objects.push(Value::Null);

            let size = u29 >> 1;
            let is_fixed = try!(self.reader.read_u8()) != 0;
            let name = try!(self.decode_utf8());
            let entries = try!((0..size).map(|_| self.decode_value()).collect());

            let value = Value::ObjectVector {
                name: if name == "*" { None } else { Some(name) },
                is_fixed: is_fixed,
                entries: entries,
            };

            self.objects[index] = value.clone();
            Ok(value)
        }
    }

    fn decode_dictionary(&mut self) -> DecodeResult<Value> {
        let u29 = try!(self.decode_u29()) as usize;
        let is_reference = (u29 & 0x01) == 0;

        if is_reference {
            let index = u29 >> 1;
            self.objects
                .get(index)
                .ok_or(DecodeError::NotFoundInReferenceTable { index: index })
                .and_then(|v| Ok(v.clone()))
        } else {
            let index = self.objects.len();
            self.objects.push(Value::Null);

            let size = u29 >> 1;
            let is_weak = try!(self.reader.read_u8()) == 1;
            let entries = try!(
                (0..size)
                    .map(|_| {
                        Ok(Pair {
                            key: try!(self.decode_value()),
                            value: try!(self.decode_value()),
                        })
                    })
                    .collect::<DecodeResult<_>>()
            );

            let value = Value::Dictionary {
                is_weak: is_weak,
                entries: entries,
            };

            self.objects[index] = value.clone();
            Ok(value)
        }
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
            Marker::XML => self.decode_xml(),
            Marker::ARRAY => self.decode_array(),
            Marker::BYTE_ARRAY => self.decode_byte_array(),
            Marker::OBJECT => self.decode_object(),
            Marker::VECTOR_INT => self.decode_vector_int(),
            Marker::VECTOR_UINT => self.decode_vector_uint(),
            Marker::VECTOR_DOUBLE => self.decode_vector_double(),
            Marker::VECTOR_OBJECT => self.decode_vector_object(),
            Marker::DICTIONARY => self.decode_dictionary(),

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

    // 1.3.1 Variable Length Unsigned 29-bit Integer Encoding
    // AMF 3 makes use of a special compact format for writing integers to reduce the number of bytes required for encoding. As with a normal 32-bit integer, up to 4 bytes are required to hold the value however the high bit of the first 3 bytes are used as flags to determine whether the next byte is part of the integer. With up to 3 bits of the 32 bits being used as flags, only 29 significant bits remain for encoding an integer. This means the largest unsigned integer value that can be represented is 229 - 1.
    // (hex)
    // 0x00000000 - 0x0000007F
    // 0x00000080 - 0x00003FFF
    // 0x00004000 - 0x001FFFFF
    // 0x00200000 - 0x3FFFFFFF
    // 0x40000000 - 0xFFFFFFFF
    // : (binary)
    // :  0xxxxxxx
    // :  1xxxxxxx 0xxxxxxx
    // :  1xxxxxxx 1xxxxxxx 0xxxxxxx
    // :  1xxxxxxx 1xxxxxxx 1xxxxxxx xxxxxxxx
    // :  throw range exception
    // In ABNF syntax, the variable length unsigned 29-bit integer type is described as follows:
    // U29 = U29-1 | U29-2 | U29-3 | U29-4
    // U29-1 = %x00-7F
    // U29-2 = %x80-FF %x00-7F
    // U29-3 = %x80-FF %x80-FF %x00-7F
    // U29-4 = %x80-FF %x80-FF %x80-FF %x00-FF
    fn encode_u29(&mut self, u29: u32) -> EncodeResult<()> {
        if u29 < 0x80 {
            // U29-1
            try!(self.writer.write_u8(u29 as u8));
        } else if u29 < 0x4000 {
            // U29-2
            let b1 = (u29 >> 7 | 0x80) as u8;
            let b2 = (u29 & 0x7F) as u8;
            for b in &[b1, b2] {
                try!(self.writer.write_u8(*b));
            }
        } else if u29 > 0x3FFF && u29 <= 0x1FFFFF {
            // U29-3
            let b1 = (u29 >> 14 | 0x80) as u8;
            let b2 = (((u29 >> 7) & 0x7F) | 0x80) as u8;
            let b3 = (u29 & 0x7F) as u8;
            for b in &[b1, b2, b3] {
                try!(self.writer.write_u8(*b));
            }
        } else if u29 < 0x4000_0000 {
            // U29-4
            let b1 = (u29 >> 22 | 0x80) as u8;
            let b2 = (((u29 >> 15) & 0x7F) | 0x80) as u8;
            let b3 = (((u29 >> 8) & 0x7F) | 0x80) as u8;
            let b4 = (u29 & 0xFF) as u8;
            for b in &[b1, b2, b3, b4] {
                try!(self.writer.write_u8(*b));
            }
        } else {
            return Err(EncodeError::U29Overflow { u29 });
        }
        Ok(())
    }

    // TODO: reference tableのサポート
    fn encode_utf8(&mut self, s: &str) -> EncodeResult<()> {
        let size = ((s.len() << 1) | 0x01) as u32;
        try!(self.encode_u29(size));

        try!(self.writer.write_all(s.as_bytes()));
        Ok(())
    }

    fn encode_pairs(&mut self, pairs: &[Pair<String, Value>]) -> EncodeResult<()> {
        for pair in pairs {
            try!(self.encode_utf8(&pair.key));
            try!(self.encode(&pair.value));
        }
        try!(self.encode_utf8("")); // UTF-8-empty
        Ok(())
    }

    fn encode_boolean(&mut self, boolean: bool) -> EncodeResult<()> {
        if boolean {
            try!(self.writer.write_u8(Marker::TRUE));
        } else {
            try!(self.writer.write_u8(Marker::FALSE));
        }
        Ok(())
    }

    fn encode_integer(&mut self, integer: i32) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::INTEGER));
        let u29 = if integer >= 0 {
            integer as u32
        } else {
            ((1 << 29) + integer) as u32
        };
        try!(self.encode_u29(u29));
        Ok(())
    }

    fn encode_double(&mut self, double: f64) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::DOUBLE));
        try!(self.writer.write_f64::<BigEndian>(double));
        Ok(())
    }

    fn encode_string(&mut self, string: &str) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::STRING));
        try!(self.encode_utf8(string));
        Ok(())
    }

    fn encode_xml_document(&mut self, xml_doc: &str) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::XML_DOC));
        try!(self.encode_utf8(xml_doc));
        Ok(())
    }

    fn encode_date(&mut self, unixtime: time::Duration) -> EncodeResult<()> {
        let ms = unixtime.as_secs() * 1000 + (unixtime.subsec_nanos() as u64) / 1000_000;
        try!(self.writer.write_u8(Marker::DATE));
        let size = ((0 << 1) | 0x01) as u32;
        try!(self.encode_u29(size));
        try!(self.writer.write_f64::<BigEndian>(ms as f64));
        Ok(())
    }

    fn encode_xml(&mut self, xml: &str) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::XML));
        try!(self.encode_utf8(xml));
        Ok(())
    }

    // TODO: reference tableのサポート
    fn encode_object(
        &mut self,
        name: &Option<String>,
        sealed_count: usize,
        pairs: &[Pair<String, Value>],
    ) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::OBJECT));

        let is_reference = 1 as usize;
        let is_externalizable = false as usize;
        let is_dynamic = (sealed_count < pairs.len()) as usize;
        let u29 = (sealed_count << 3) | (is_dynamic << 2) | (is_externalizable << 1) | is_reference;
        let size = ((u29 << 1) | 0x01) as u32;
        try!(self.encode_u29(size));

        let name = name.as_ref().map_or("", |s| &s);
        try!(self.encode_utf8(name));
        for pair in pairs.iter().take(sealed_count) {
            try!(self.encode_utf8(&pair.key));
        }

        for pair in pairs.iter().take(sealed_count) {
            try!(self.encode(&pair.value));
        }

        if pairs.len() > sealed_count {
            try!(self.encode_pairs(&pairs[sealed_count..]));
        }

        Ok(())
    }

    // TODO: reference tableのサポート
    fn encode_array(&mut self, assoc: &[Pair<String, Value>], dense: &[Value]) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::ARRAY));
        let size = ((dense.len() << 1) | 0x01) as u32;
        try!(self.encode_u29(size));
        try!(self.encode_pairs(assoc));
        try!(
            dense
                .iter()
                .map(|v| self.encode(v))
                .collect::<EncodeResult<Vec<_>>>()
        );
        Ok(())
    }

    // TODO: reference tableのサポート
    fn encode_byte_array(&mut self, bytes: &[u8]) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::BYTE_ARRAY));
        let size = ((bytes.len() << 1) | 0x01) as u32;
        try!(self.encode_u29(size));
        try!(self.writer.write_all(bytes));
        Ok(())
    }

    // TODO: reference tableのサポート
    fn encode_vector_int(&mut self, is_fixed: bool, vec: &[i32]) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::VECTOR_INT));
        let size = ((vec.len() << 1) | 0x01) as u32;
        try!(self.encode_u29(size));
        try!(self.writer.write_u8(is_fixed as u8));
        for &v in vec {
            try!(self.writer.write_i32::<BigEndian>(v));
        }
        Ok(())
    }

    // TODO: reference tableのサポート
    fn encode_vector_uint(&mut self, is_fixed: bool, vec: &[u32]) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::VECTOR_UINT));
        let size = ((vec.len() << 1) | 0x01) as u32;
        try!(self.encode_u29(size));
        try!(self.writer.write_u8(is_fixed as u8));
        for &v in vec {
            try!(self.writer.write_u32::<BigEndian>(v));
        }
        Ok(())
    }

    // TODO: reference tableのサポート
    fn encode_vector_double(&mut self, is_fixed: bool, vec: &[f64]) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::VECTOR_DOUBLE));
        let size = ((vec.len() << 1) | 0x01) as u32;
        try!(self.encode_u29(size));
        try!(self.writer.write_u8(is_fixed as u8));
        for &v in vec {
            try!(self.writer.write_f64::<BigEndian>(v));
        }
        Ok(())
    }

    // TODO: reference tableのサポート
    fn encode_vector_object(
        &mut self,
        name: &Option<String>,
        is_fixed: bool,
        vec: &[Value],
    ) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::VECTOR_OBJECT));
        let size = ((vec.len() << 1) | 0x01) as u32;
        try!(self.encode_u29(size));
        try!(self.writer.write_u8(is_fixed as u8));
        try!(self.encode_utf8(name.as_ref().map_or("*", |s| &s)));
        for v in vec {
            try!(self.encode(v));
        }
        Ok(())
    }

    // TODO: reference tableのサポート
    fn encode_dictionary(
        &mut self,
        is_weak: bool,
        pairs: &[Pair<Value, Value>],
    ) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::DICTIONARY));
        let size = ((pairs.len() << 1) | 0x01) as u32;
        try!(self.encode_u29(size));
        try!(self.writer.write_u8(is_weak as u8));
        for pair in pairs {
            try!(self.encode(&pair.key));
            try!(self.encode(&pair.value));
        }
        Ok(())
    }

    fn encode_undefined(&mut self) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::UNDEFINED));
        Ok(())
    }

    fn encode_null(&mut self) -> EncodeResult<()> {
        try!(self.writer.write_u8(Marker::NULL));
        Ok(())
    }

    fn encode_value(&mut self, value: &Value) -> EncodeResult<()> {
        match *value {
            Value::Undefined => self.encode_undefined(),
            Value::Null => self.encode_null(),
            Value::Boolean(boolean) => self.encode_boolean(boolean),
            Value::Integer(integer) => self.encode_integer(integer),
            Value::Double(double) => self.encode_double(double),
            Value::String(ref string) => self.encode_string(string),
            Value::XmlDoc(ref xml_doc) => self.encode_xml_document(xml_doc),
            Value::Date { unixtime } => self.encode_date(unixtime),
            Value::Object {
                ref name,
                sealed_count,
                ref pairs,
            } => self.encode_object(name, sealed_count, pairs),
            Value::Xml(ref xml) => self.encode_xml(xml),
            Value::Array {
                ref assoc_entries,
                ref dense_entries,
            } => self.encode_array(assoc_entries, dense_entries),
            Value::ByteArray(ref bytes) => self.encode_byte_array(bytes),
            Value::IntVector {
                is_fixed,
                ref entries,
            } => self.encode_vector_int(is_fixed, entries),
            Value::UintVector {
                is_fixed,
                ref entries,
            } => self.encode_vector_uint(is_fixed, entries),
            Value::DoubleVector {
                is_fixed,
                ref entries,
            } => self.encode_vector_double(is_fixed, entries),
            Value::ObjectVector {
                ref name,
                is_fixed,
                ref entries,
            } => self.encode_vector_object(name, is_fixed, entries),
            Value::Dictionary {
                is_weak,
                ref entries,
            } => self.encode_dictionary(is_weak, entries),
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
    use super::Pair;
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
        let expected = Value::Object {
            name: Some("com.pyyoshi.nodynamichogeclass".to_string()),
            sealed_count: 0,
            pairs: vec![],
        };
        macro_decode_equal!("amf3-object.bin", expected);

        let expected = Value::Object {
            name: Some("com.pyyoshi.hogeclass".to_string()),
            sealed_count: 2,
            pairs: vec![
                Pair {
                    key: "index".to_string(),
                    value: Value::Integer(0),
                },
                Pair {
                    key: "msg".to_string(),
                    value: Value::String("fugaaaaaaa".to_string()),
                },
            ],
        };
        macro_decode_equal!("amf3-object-ref.bin", expected);

        let expected = Value::Object {
            name: Some("com.pyyoshi.dynamichogeclass".to_string()),
            sealed_count: 2,
            pairs: vec![
                Pair {
                    key: "index".to_string(),
                    value: Value::Integer(0),
                },
                Pair {
                    key: "msg".to_string(),
                    value: Value::String("fugaaaaaaa".to_string()),
                },
            ],
        };
        macro_decode_equal!("amf3-object-dynamic.bin", expected);

        let expected = Value::Object {
            name: Some("com.pyyoshi.nodynamichogeclass".to_string()),
            sealed_count: 0,
            pairs: vec![],
        };
        macro_decode_equal!("amf3-object-typed.bin", expected);

        let expected = Value::Object {
            name: None,
            sealed_count: 2,
            pairs: vec![
                Pair {
                    key: "index".to_string(),
                    value: Value::Integer(0),
                },
                Pair {
                    key: "msg".to_string(),
                    value: Value::String("fugaaaaaaa".to_string()),
                },
            ],
        };
        macro_decode_equal!("amf3-object-hash.bin", expected);
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
        let expected1 = Value::Array {
            assoc_entries: vec![
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
            dense_entries: vec![],
        };
        macro_decode_equal!("amf3-array-assoc.bin", expected1);

        let expected2 = Value::Array {
            assoc_entries: vec![],
            dense_entries: vec![
                Value::Double(1.1_f64),
                Value::Integer(2_i32),
                Value::Double(3.3_f64),
                Value::String("こんにちは、世界！".to_string()),
            ],
        };
        macro_decode_equal!("amf3-array-dense.bin", expected2);
    }

    #[test]
    fn decode_byte_array() {
        let expected = Value::ByteArray("hello".as_bytes().iter().cloned().collect());
        macro_decode_equal!("amf3-byte-array.bin", expected);
    }

    #[test]
    fn decode_vector_int() {
        let expected = Value::IntVector {
            is_fixed: false,
            entries: vec![-1, 0, 1],
        };
        macro_decode_equal!("amf3-vector-int.bin", expected);
    }

    #[test]
    fn decode_vector_uint() {
        let expected = Value::UintVector {
            is_fixed: false,
            entries: vec![0, 1, 2],
        };
        macro_decode_equal!("amf3-vector-uint.bin", expected);
    }

    #[test]
    fn decode_vector_double() {
        let expected = Value::DoubleVector {
            is_fixed: false,
            entries: vec![-1.1_f64, 0_f64, 1.1_f64],
        };
        macro_decode_equal!("amf3-vector-double.bin", expected);
    }

    #[test]
    fn decode_vector_object() {
        let expected = Value::ObjectVector {
            name: Some("com.pyyoshi.fooclass".to_string()),
            is_fixed: false,
            entries: vec![
                Value::Object {
                    name: None,
                    sealed_count: 2,
                    pairs: vec![
                        Pair {
                            key: "index".to_string(),
                            value: Value::Integer(0),
                        },
                        Pair {
                            key: "msg".to_string(),
                            value: Value::String("Hello, world!".to_string()),
                        },
                    ],
                },
                Value::Object {
                    name: None,
                    sealed_count: 2,
                    pairs: vec![
                        Pair {
                            key: "index".to_string(),
                            value: Value::Integer(1),
                        },
                        Pair {
                            key: "msg".to_string(),
                            value: Value::String("こんにちは、世界！".to_string()),
                        },
                    ],
                },
                Value::Object {
                    name: None,
                    sealed_count: 2,
                    pairs: vec![
                        Pair {
                            key: "index".to_string(),
                            value: Value::Integer(2),
                        },
                        Pair {
                            key: "msg".to_string(),
                            value: Value::String("你好世界".to_string()),
                        },
                    ],
                },
            ],
        };
        macro_decode_equal!("amf3-vector-object.bin", expected);
    }

    #[test]
    fn decode_dictionary() {
        let expected = Value::Dictionary {
            is_weak: false,
            entries: vec![
                Pair {
                    key: Value::String("en".to_string()),
                    value: Value::String("Hello, world!".to_string()),
                },
                Pair {
                    key: Value::String("ja".to_string()),
                    value: Value::String("こんにちは、世界！".to_string()),
                },
                Pair {
                    key: Value::String("zh".to_string()),
                    value: Value::String("你好世界".to_string()),
                },
            ],
        };
        macro_decode_equal!("amf3-dictionary.bin", expected);
    }

    #[test]
    fn encode_integer() {
        macro_encode_equal!(Value::Integer(0), "amf3-integer-0.bin");
        macro_encode_equal!(Value::Integer(128), "amf3-integer-128.bin");
        macro_encode_equal!(Value::Integer(16384), "amf3-integer-16384.bin");
        macro_encode_equal!(Value::Integer(268435455), "amf3-integer-max-u29.bin");
        macro_encode_equal!(Value::Integer(-268435456), "amf3-integer-min-u29.bin");
    }

    #[test]
    fn encode_double() {
        macro_encode_equal!(Value::Double(268435456_f64), "amf3-double-max-u29.bin");
        macro_encode_equal!(Value::Double(-268435457_f64), "amf3-double-min-u29.bin");
        macro_encode_equal!(Value::Double(3.14_f64), "amf3-double-pi.bin");
    }

    #[test]
    fn encode_string() {
        macro_encode_equal!(
            Value::String("こんにちは、世界！".to_string()),
            "amf3-string.bin"
        );
    }

    #[test]
    fn encode_xml_doc() {
        macro_encode_equal!(
            Value::XmlDoc("<a><b>hello world</b></a>".to_string()),
            "amf3-xml-doc.bin"
        );
    }

    #[test]
    fn encode_date() {
        macro_encode_equal!(
            Value::Date { unixtime: time::Duration::from_millis(1111111111_000) },
            "amf3-date.bin"
        );
    }

    #[test]
    fn encode_object() {
        let value = Value::Object {
            name: None,
            sealed_count: 0,
            pairs: vec![
                Pair {
                    key: "index".to_string(),
                    value: Value::Integer(0),
                },
                Pair {
                    key: "msg".to_string(),
                    value: Value::String("fugaaaaaaa".to_string()),
                },
            ],
        };
        macro_encode_equal!(value, "amf3-object-hash.bin");

        let value = Value::Object {
            name: Some("com.pyyoshi.nodynamichogeclass".to_string()),
            sealed_count: 0,
            pairs: vec![],
        };
        macro_encode_equal!(value, "amf3-object-typed.bin");
    }

    #[test]
    fn encode_xml() {
        macro_encode_equal!(
            Value::Xml("<a><b>hello world</b></a>".to_string()),
            "amf3-xml.bin"
        );
    }

    #[test]
    fn encode_undefined() {
        macro_encode_equal!(Value::Undefined, "amf3-undefined.bin");
    }

    #[test]
    fn encode_null() {
        macro_encode_equal!(Value::Null, "amf3-null.bin");
    }

    #[test]
    fn encode_boolean() {
        macro_encode_equal!(Value::Boolean(false), "amf3-boolean-false.bin");
        macro_encode_equal!(Value::Boolean(true), "amf3-boolean-true.bin");
    }

    #[test]
    fn encode_array() {
        let value = Value::Array {
            assoc_entries: vec![
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
            dense_entries: vec![],
        };
        macro_encode_equal!(value, "amf3-array-assoc.bin");

        let value = Value::Array {
            assoc_entries: vec![],
            dense_entries: vec![
                Value::Double(1.1_f64),
                Value::Integer(2_i32),
                Value::Double(3.3_f64),
                Value::String("こんにちは、世界！".to_string()),
            ],
        };
        macro_encode_equal!(value, "amf3-array-dense.bin");
    }

    #[test]
    fn encode_byte_array() {
        let value = Value::ByteArray("hello".as_bytes().iter().cloned().collect());
        macro_encode_equal!(value, "amf3-byte-array.bin");
    }

    #[test]
    fn encode_vector_int() {
        let value = Value::IntVector {
            is_fixed: false,
            entries: vec![-1, 0, 1],
        };
        macro_encode_equal!(value, "amf3-vector-int.bin");
    }

    #[test]
    fn encode_vector_uint() {
        let value = Value::UintVector {
            is_fixed: false,
            entries: vec![0, 1, 2],
        };
        macro_encode_equal!(value, "amf3-vector-uint.bin");
    }

    #[test]
    fn encode_vector_double() {
        let value = Value::DoubleVector {
            is_fixed: false,
            entries: vec![-1.1_f64, 0_f64, 1.1_f64],
        };
        macro_encode_equal!(value, "amf3-vector-double.bin");
    }

    #[test]
    fn encode_vector_object() {
        let value = Value::ObjectVector {
            name: Some("com.pyyoshi.fooclass".to_string()),
            is_fixed: false,
            entries: vec![
                Value::Object {
                    name: None,
                    sealed_count: 2,
                    pairs: vec![
                        Pair {
                            key: "index".to_string(),
                            value: Value::Integer(0),
                        },
                        Pair {
                            key: "msg".to_string(),
                            value: Value::String("Hello, world!".to_string()),
                        },
                    ],
                },
                Value::Object {
                    name: None,
                    sealed_count: 2,
                    pairs: vec![
                        Pair {
                            key: "index".to_string(),
                            value: Value::Integer(1),
                        },
                        Pair {
                            key: "msg".to_string(),
                            value: Value::String("こんにちは、世界！".to_string()),
                        },
                    ],
                },
                Value::Object {
                    name: None,
                    sealed_count: 2,
                    pairs: vec![
                        Pair {
                            key: "index".to_string(),
                            value: Value::Integer(2),
                        },
                        Pair {
                            key: "msg".to_string(),
                            value: Value::String("你好世界".to_string()),
                        },
                    ],
                },
            ],
        };

        let mut buf = Vec::new();
        let _ = Encoder::new(&mut buf).encode(&value);
        let result = Decoder::new(&mut &buf[..]).decode().unwrap();
        assert_eq!(value, result);
    }

    #[test]
    fn encode_dictionary() {
        let value = Value::Dictionary {
            is_weak: false,
            entries: vec![
                Pair {
                    key: Value::String("en".to_string()),
                    value: Value::String("Hello, world!".to_string()),
                },
                Pair {
                    key: Value::String("ja".to_string()),
                    value: Value::String("こんにちは、世界！".to_string()),
                },
                Pair {
                    key: Value::String("zh".to_string()),
                    value: Value::String("你好世界".to_string()),
                },
            ],
        };
        macro_encode_equal!(value, "amf3-dictionary.bin");
    }
}
