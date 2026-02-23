use std::str::FromStr;
use std::fmt::Display;
use crate::errors::ErrType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockQuote {
    pub ticker: String,
    pub price: u32, // сделал целым числом что бы не мучиться с колчиством знаков после запятой. Буду считать что хранимая 12345 число интерпретируется как 123.45
    pub volume: u32,
    pub timestamp: u64,
}

impl StockQuote {
    // Или бинарная сериализация
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(self.ticker.as_bytes());
        bytes.push(b'|');
        bytes.extend_from_slice(self.price.to_string().as_bytes());
        bytes.push(b'|');
        bytes.extend_from_slice(self.volume.to_string().as_bytes());
        bytes.push(b'|');
        bytes.extend_from_slice(self.timestamp.to_string().as_bytes());
        bytes
    }
}

impl FromStr for StockQuote {
    type Err = ErrType;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('|').collect();
        if parts.len() == 4 {
            Ok(StockQuote {
                ticker: parts[0].to_string(),
                price: parts[1].replace(".", "").parse::<u32>()?,
                volume: parts[2].parse()?,
                timestamp: parts[3].parse()?,
            })
        } else {
            Err(ErrType::NotSupported(format!("Не удалось прочитать котировку из строку {}",  s.to_string())))
        }
    }
}

impl Display for StockQuote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}|{}.{:02}|{}|{}", self.ticker, self.price / 100, self.price % 100, self.volume, self.timestamp)
    }
}