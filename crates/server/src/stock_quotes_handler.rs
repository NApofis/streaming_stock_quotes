use common_lib::errors::ErrType;
use common_lib::stock_quote::StockQuote;
use rand::Rng;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{thread, thread::JoinHandle};
use std::sync::atomic::{AtomicBool, Ordering};
use crossbeam_channel::{unbounded, Receiver, Sender};
use common_lib::errors::ErrType::NoAccess;

const POPULAR_QUOTES: [&str; 3] = ["AAPL", "MSFT", "TSLA"];


pub struct QuoteHandler {
    stopper: Arc<AtomicBool>,
    join_handle: Option<JoinHandle<()>>,
    receiver: Receiver<Arc<Vec<StockQuote>>>
}

impl QuoteHandler {
    /// Создаст объект и запустит поток обновления котировок
    ///
    /// # Arguments
    ///
    /// * `tickers`: Список имен котировок для которых необходимо обновлять значения
    ///
    /// returns: (QuoteHandler, JoinHandle<()>) - Объект и поток для корректной завершении работы
    ///
    pub fn new(tickers: &HashSet<String>) -> QuoteHandler {
        let stopper = Arc::new(AtomicBool::new(false));
        let stopper_clone = stopper.clone();
        let (sender, receiver) = unbounded::<Arc<Vec<StockQuote>>>();

        Self {
            stopper,
            join_handle: Some(Self::start_update_quotes(stopper_clone, tickers, sender)),
            receiver
        }
    }

    /// Запускаем поток, который будет обновлять котировки, что бы у чтецов были свежие данные
    ///
    /// # Arguments
    ///
    /// * `inner`: Ссылка на общие данные где и будут храниться данные котировок
    ///
    /// returns: JoinHandle<()> - держатель потока с помощью которого можно будет дождаться корректного завершения потока
    ///
    fn start_update_quotes(stopper: Arc<AtomicBool>, tickers: &HashSet<String>, sender: Sender<Arc<Vec<StockQuote>>>) -> JoinHandle<()> {
        let mut stocks: Vec<StockQuote> = Vec::new();
        for ticker in tickers {
            stocks.push(Self::generate_quote(ticker, None));
        }
        let mut counter: i32 = 0;

        thread::spawn(move || {
            log::info!("Запущен поток обновления котировок");

            loop {
                if stopper.load(Ordering::Acquire) {
                    log::info!("Остановлен поток обновления котировок");
                    break;
                }

                for quote in &mut stocks {
                    let new = Self::generate_quote(&quote.ticker, Some(quote.price));
                    quote.price = new.price;
                    quote.volume = new.volume;
                    quote.timestamp = new.timestamp;
                }

                match sender.try_send(Arc::new(stocks.clone())) {
                    Ok(_) => {
                        counter = 0
                    },
                    Err(_) => {
                        if counter > 10 {
                            counter = 0;
                            log::debug!(
                                "Не удалось отправить данные котировок в канал"
                            );
                        }
                        counter += 1;
                    }
                }

                thread::sleep(Duration::new(1, 0));
            }
        })
    }

    /// Остановит работу потока обновляющего значения котировок
    pub fn stop(&mut self) -> Result<(), ErrType> {
        self.stopper.store(true, Ordering::Release);
        log::info!("Поток обновления значений котировок будет остановлен");
        if let Some(join_handle) = self.join_handle.take() {
            match join_handle.join() {
                Ok(()) => (),
                Err(_) => {
                    log::error!("Ошибка завершения работы потока обновления котировок");
                    return Err(NoAccess("Не удалось завершить работу потока обновляющего значения котировок".to_string()));
                }
            }
        }
        Ok(())
    }
    
    pub fn get_receiver(&self) -> Receiver<Arc<Vec<StockQuote>>> {
        self.receiver.clone()
    }

    /// Генерирует новое значение для котировки. Изначально берется рандомная цена, а в последующих вызовах
    /// цена генерируется в промежутке 80% от предыдущей цены до 120% от предыдущей цены. В результате изменения
    /// цены будут реалистичнее
    ///
    /// # Arguments
    ///
    /// * `ticker`: название котировки
    /// * `last_price`: предыдущая цена которая будет None при первом вызове
    ///
    /// returns: Option<StockQuote> - новая котировка
    ///
    fn generate_quote(ticker: &str, last_price: Option<u32>) -> StockQuote {
        let mut generator = rand::rng();
        let price: u32;
        if let Some(lp) = last_price {
            let start;
            let end;
            if lp < 100 {
                start = lp;
                end = lp * 2;
            } else {
                let percents = (lp / 100) * 20;
                start = lp - percents;
                end = lp + percents;
            }
            price = generator.random_range(start..end);
        } else {
            price = generator.random_range(10..100000);
        }

        let volume = if POPULAR_QUOTES.contains(&ticker) {
            // Популярные акции имеют больший объём
            1000 + (rand::random::<f64>() * 5000.0) as u32
        } else {
            // Обычные акции - средний объём
            100 + (rand::random::<f64>() * 1000.0) as u32
        };

        StockQuote {
            ticker: ticker.to_string(),
            price,
            volume,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use std::collections::HashSet;
//     use std::thread::sleep;
//     use std::time::Duration;
//     use crate::stock_quotes_handler::QuoteHandler;
// 
//     fn get_tickers() -> HashSet<String> {
//         HashSet::from(["AAPL".to_string(), "MSFT".to_string(), "NEW".to_string()])
//     }
// 
//     #[test]
//     fn test_start_stop_generate_quote() {
//         let (obj, thr) = QuoteHandler::new(&get_tickers());
//         {
//             let reader = obj.inner.read().unwrap();
//             assert!(reader.stocks.iter().find(|s| s.ticker.as_str() == "AAPL").is_some());
//             assert!(reader.stocks.iter().find(|s| s.ticker.as_str() == "MSFT").is_some());
//             assert!(reader.stocks.iter().find(|s| s.ticker.as_str() == "NEWWW").is_none());
//         }
// 
//         let mut tr = get_tickers();
//         tr.insert(String::from("GGL"));
//         let value = obj.read(&tr).unwrap();
// 
//         sleep(Duration::from_secs(2));
//         let value2 = obj.read(&tr).unwrap();
// 
//         assert_eq!(value.len(), 3);
//         assert_eq!(value.len(), value2.len());
//         assert_eq!(value[0].ticker, value2[0].ticker);
//         assert_eq!(value[1].ticker, value2[1].ticker);
//         assert_eq!(value[2].ticker, value2[2].ticker);
// 
//         assert_ne!(value[0].timestamp, value2[0].timestamp);
//         assert_ne!(value[1].timestamp, value2[1].timestamp);
//         assert_ne!(value[2].timestamp, value2[2].timestamp);
// 
//         assert_ne!(value[0].price, value2[0].price);
//         assert_ne!(value[0].volume, value2[0].volume);
// 
//         obj.stop();
//         let empty_result = obj.read(&tr).unwrap();
//         assert!(empty_result.is_empty());
//         thr.join().unwrap();
//     }
// }