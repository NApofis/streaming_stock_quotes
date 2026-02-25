use common_lib::errors::ErrType;
use common_lib::errors::ErrType::NoAccess;
use common_lib::stock_quote::StockQuote;
use crossbeam_channel::{Receiver, Sender, unbounded};
use rand::Rng;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{thread, thread::JoinHandle};

const POPULAR_QUOTES: [&str; 3] = ["AAPL", "MSFT", "TSLA"];
type SubsType = Arc<RwLock<HashMap<String, Sender<Arc<Vec<StockQuote>>>>>>;

pub struct QuoteHandler {
    stopper: Arc<AtomicBool>,
    join_handle: Option<JoinHandle<()>>,
    subscribers: SubsType,
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
        let subscribers = Arc::new(RwLock::new(HashMap::new()));

        Self {
            stopper,
            join_handle: Some(Self::start_update_quotes(
                stopper_clone,
                tickers,
                subscribers.clone(),
            )),
            subscribers,
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
    fn start_update_quotes(
        stopper: Arc<AtomicBool>,
        tickers: &HashSet<String>,
        subscribers: SubsType,
    ) -> JoinHandle<()> {
        let mut stocks: Vec<StockQuote> = Vec::new();
        for ticker in tickers {
            stocks.push(Self::generate_quote(ticker, None));
        }

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

                let data = Arc::new(stocks.clone());
                match subscribers.read() {
                    Ok(subscribers) => {
                        subscribers
                            .iter()
                            .for_each(|(_, s)| match s.try_send(data.clone()) {
                                Ok(_) => {}
                                Err(e) => {
                                    log::warn!("Не удалось отправить сообщение по каналу. {:?}", e)
                                }
                            })
                    }
                    Err(_) => {
                        log::debug!("Не удалось отправить данные котировок в канал");
                    }
                };
                thread::sleep(common_lib::QUOTE_GENERATOR_PERIOD);
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
                    return Err(NoAccess(
                        "Не удалось завершить работу потока обновляющего значения котировок"
                            .to_string(),
                    ));
                }
            }
        }
        Ok(())
    }

    /// Создаем новое канал по которому будем отправлять котировки
    pub fn create_channel(&self, address: &String) -> Option<Receiver<Arc<Vec<StockQuote>>>> {
        match self.subscribers.write() {
            Ok(mut subscribers) => {
                let (sender, receiver) = unbounded::<Arc<Vec<StockQuote>>>();
                subscribers.insert(address.to_string(), sender);
                Some(receiver)
            }
            Err(_) => {
                log::error!("Не удалось создать канал для передачи котировок");
                None
            }
        }
    }

    pub fn remove_channel(&self, address: &String) {
        match self.subscribers.write() {
            Ok(mut subscribers) => {
                subscribers.remove(address);
            }
            Err(_) => {
                log::error!("Не удалось удалить канал для передачи котировок");
            }
        }
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
                .as_millis() as i64,
        }
    }
}
