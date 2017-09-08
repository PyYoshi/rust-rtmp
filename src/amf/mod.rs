use std::{error, fmt, io, string};

pub enum Version {
    AMF0,
    AMF3,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pair<K, V> {
    pub key: K,
    pub value: V,
}

#[derive(Debug)]
pub enum DecodeError {
    Io(io::Error),
    String(string::FromUtf8Error),
    NotSupportedType { marker: u8 },
    NotExpectedObjectEnd,
    UnknownType { marker: u8 },
    NotSupportedReferenceTables { index: usize },
    NotFoundInReferenceTable { index: usize },
    ExternalizableType { name: String },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            DecodeError::Io(ref x) => write!(f, "I/O Error: {}", x),
            DecodeError::String(ref x) => write!(f, "Invalid String: {}", x),
            DecodeError::NotSupportedType { marker } => {
                write!(f, "Not supported type: marker={}", marker)
            }
            DecodeError::NotExpectedObjectEnd => {
                write!(f, "Not expected occurrence of object-end-marker")
            }
            DecodeError::UnknownType { marker } => write!(f, "Unknown type: maker={}", marker),
            DecodeError::NotSupportedReferenceTables { index } => {
                write!(f, "Reference Tables is not supported: index={}", index)
            }
            DecodeError::NotFoundInReferenceTable { index } => {
                write!(f, "Value is not found in reference table: index={}", index)
            }
            DecodeError::ExternalizableType { ref name } => {
                write!(f, "Externalizable type {:?} is unsupported", name)
            }
        }
    }
}

impl error::Error for DecodeError {
    fn description(&self) -> &str {
        match *self {
            DecodeError::Io(ref x) => x.description(),
            DecodeError::String(ref x) => x.description(),
            DecodeError::NotSupportedType { .. } => "Not supported type",
            DecodeError::NotExpectedObjectEnd { .. } => {
                "Unexpected occurrence of object-end-marker"
            }
            DecodeError::UnknownType { .. } => "Unknown type",
            DecodeError::NotSupportedReferenceTables { .. } => "Reference Tables is not supported",
            DecodeError::NotFoundInReferenceTable { .. } => "Value is not found in reference table",
            DecodeError::ExternalizableType { .. } => "Unsupported externalizable type",
        }
    }

    fn cause(&self) -> Option<&error::Error> {
        match *self {
            DecodeError::Io(ref x) => x.cause(),
            DecodeError::String(ref x) => x.cause(),
            _ => None,
        }
    }
}

impl PartialEq for DecodeError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (&DecodeError::UnknownType { marker: x }, &DecodeError::UnknownType { marker: y }) => {
                x == y
            }
            (&DecodeError::NotSupportedType { marker: x },
             &DecodeError::NotSupportedType { marker: y }) => x == y,
            (&DecodeError::NotExpectedObjectEnd, &DecodeError::NotExpectedObjectEnd) => true,
            (&DecodeError::NotSupportedReferenceTables { index: x },
             &DecodeError::NotSupportedReferenceTables { index: y }) => x == y,
            (&DecodeError::NotFoundInReferenceTable { index: x },
             &DecodeError::NotFoundInReferenceTable { index: y }) => x == y,
            (&DecodeError::ExternalizableType { name: ref x },
             &DecodeError::ExternalizableType { name: ref y }) => x == y,
            _ => false,
        }
    }
}

impl From<io::Error> for DecodeError {
    fn from(f: io::Error) -> Self {
        DecodeError::Io(f)
    }
}

impl From<string::FromUtf8Error> for DecodeError {
    fn from(f: string::FromUtf8Error) -> Self {
        DecodeError::String(f)
    }
}

pub type DecodeResult<T> = Result<T, DecodeError>;

pub mod amf0;
pub mod amf3;
