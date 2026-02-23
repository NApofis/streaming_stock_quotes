use std::collections::HashSet;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use common_lib::errors::ErrType;
use common_lib::stock_quote::StockQuote;
use common_lib::{PING_REQUEST, DATA_REQUEST, PONG_REQUEST};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use std::io;


pub struct ClientReader {
    socket: UdpSocket,
    tickers: HashSet<String>,
    address: String,
    stop: Arc<AtomicBool>,
    ping_status: Arc<AtomicBool>,
    stop_without_pong: Arc<AtomicBool>,
}

impl ClientReader {
    pub fn new(address: String, tickers: HashSet<String>, stop: Arc<AtomicBool>) -> Result<Self, ErrType> {
        let socket = match UdpSocket::bind(address.clone()) {
            Ok(socket) => socket,
            Err(e) => {
                log::error!("Не удалось запусить udp сервер на сокете {address}. {e}");
                return Err(ErrType::ConnectionError("Не удолось запусить upd сервер".to_string()));
            }
        };
        let Ok(_) = socket.set_nonblocking(true) else {
            log::error!("Не удалось сделать upd сокет c {} не блокирующимся", address.clone());
            return Err(ErrType::ConnectionError(format!("Не удалось сделать upd сокет c {} не блокирующимся", address)));
        };

        Ok( Self {
            socket,
            tickers,
            address,
            stop,
            ping_status: Arc::new(AtomicBool::new(false)),
            stop_without_pong: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn start(&mut self) -> Result<(), ErrType> {
        let mut buf = [0u8; 2048];
        const TIMEOUT: Duration = Duration::from_millis(100);

        let mut deadline = Instant::now() + TIMEOUT;

        loop {

            if self.stop.load(Ordering::Acquire) {
                log::info!("Закрываем соединение");
                return Ok(());
            } else if self.stop_without_pong.load(Ordering::Acquire) {
                log::info!("Закрываем соединение потому что сервер не возвращает PONG ответ на PING");
                return Ok(());
            }

            match self.socket.recv_from(&mut buf) {
                Ok((n, from)) => {
                    let t = String::from_utf8_lossy(&buf[..n]);
                    if &buf[..DATA_REQUEST.len()] == DATA_REQUEST {
                        log::error!("От сервера пришел ответ {}", String::from_utf8_lossy(&buf[..n]));
                        deadline = Instant::now() + TIMEOUT;

                    } else if &buf[..PONG_REQUEST.len()] == PONG_REQUEST {
                        log::error!("От сервера пришел ответ {}", String::from_utf8_lossy(&buf[..n]));
                        self.ping_status.store(false, Ordering::Release);
                    }
                    else {
                        log::error!("От сервера пришел неизвестный запрос {}", String::from_utf8_lossy(&buf[..n]));
                        continue;
                    }

                    match bincode::deserialize::<Vec<StockQuote>>(&buf[DATA_REQUEST.len()..n]) {
                        Ok(quotes) => {
                            if self.tickers.len() != quotes.len() {
                                log::error!("Сервер не вернул не все запрашиваетмые значения Зарашивали: {}; Пришло:{}",
                                    self.tickers.iter().map(String::as_str).collect::<Vec<_>>().join(","),
                                    quotes.iter().map(|x| x.ticker.as_str()).collect::<Vec<_>>().join(",")
                                );
                            }
                            for quote in quotes {
                                if !self.tickers.contains(&quote.ticker) {
                                    log::error!("Сервер не вернул незапрашиваемое значение {}", quote.ticker);
                                }
                                println!("{}", quote);
                            }
                        }
                        Err(e) => {
                            log::error!("Ошибка десериализации ответа: {} {}", e, String::from_utf8_lossy(&buf[..n]));
                        }
                    }
                },
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        log::warn!("Сервер {} не прислал котировки за послединюю секунду", self.address);
                        deadline += TIMEOUT;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                Err(e) => {
                    log::error!("Произошла ошибка при получении собщений от сервера {}", self.address);
                    return Err(ErrType::RequestError(format!("Произошла ошибка при получении собщений от сервера {}", e.to_string())));
                }
            }
        }
    }

    pub fn ping_sender(&self) -> Result<(), ErrType> {
        let Ok(copy_socket) = self.socket.try_clone() else {
            Err(ErrType::ConnectionError("Не удалось создать копию сокета для отправки PING сообщений".to_string()))?
        };
        let address: String = self.address.clone();
        let ping_send = Arc::clone(&self.ping_status);

        let copy_stop = Arc::clone(&self.stop);
        let copy_stop_without_pong = Arc::clone(&self.stop_without_pong);

        thread::spawn(move || {
            loop {
                // if ping_send.load(Ordering::Acquire) {
                //     log::error!("Сервер {} не прислал PONG в течении 1 секунды. Соедине будет переоткрыто.", address);
                //     copy_stop_without_pong.store(true, Ordering::Release);
                //     break
                // } else if copy_stop.load(Ordering::Acquire) {
                //     log::info!("Завершаем отправлять ping запросы");
                //     break
                // };

                let Ok(_) = copy_socket.send_to(PING_REQUEST, address.clone()) else {
                    log::error!("Не удалось отправить PING сообщение на адрес {}", address);
                    break
                };

                ping_send.store(true, Ordering::Release);
                thread::sleep(Duration::from_secs(1));
            }
        });
        Ok(())
    }
}