use std::num::{ParseFloatError, ParseIntError};
use std::io::ErrorKind;
use std::fmt;
use crate::errors::ErrType::{ConnectionError, CtrlcError, NoAccess, NotSupported, ParseError, ReadError, RequestError};

#[derive(Debug)]
pub enum ErrType {
    NotSupported(String),
    ParseError(String),
    NoAccess(String),
    ReadError(String),
    ConnectionError(String),
    RequestError(String),
    CtrlcError(String),
}

impl fmt::Display for ErrType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self { 
            NotSupported(s) => write!(f, "Not supported({})", s),
            ParseError(s) => write!(f, "ParseError({})", s),
            NoAccess(s) => write!(f, "NoAccess({})", s),
            ReadError(s) => write!(f, "ReadError({})", s),
            ConnectionError(s) => write!(f, "ConnectionError({})", s),
            RequestError(s) => write!(f, "RequestError({})", s),
            CtrlcError(s) => write!(f, "CtrlcError({})", s),
        }
    }
}

impl From<ParseFloatError> for ErrType {
    fn from(value: ParseFloatError) -> Self {
        ParseError(format!("Ошибка парсинга данных {}", value))
    }
}

impl From<ParseIntError> for ErrType {
    fn from(value: ParseIntError) -> Self {
        ParseError(format!("Ошибка парсинга данных {}", value))
    }
}

impl From<ErrType> for std::io::Error {
    fn from(value: ErrType) -> Self {
        std::io::Error::new(ErrorKind::Other, value.to_string())
    }
}
