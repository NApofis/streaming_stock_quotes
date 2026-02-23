use std::net::UdpSocket;
use std::io;
use common_lib::errors::ErrType;
use std::{thread, thread::JoinHandle};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use crossbeam_channel::{Receiver, RecvTimeoutError};
use common_lib::errors::ErrType::NoAccess;
use common_lib::stock_quote::StockQuote;
use common_lib::{DATA_REQUEST, PONG_REQUEST, PING_REQUEST};

pub struct ServerWriter {
    stop: Arc<AtomicBool>,
    addr: String,
    join_handle: Option<JoinHandle<()>>
}

impl ServerWriter {
    pub fn start(addr: String, tickers: Vec<String>, receiver: Receiver<Arc<Vec<StockQuote>>>) -> Result<Self, ErrType> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();

        let mut result = Self {
            stop,
            addr: addr.clone(),
            join_handle: None
        };

        result.join_handle = Some(thread::spawn(move || {
            Self::send(stop_clone, addr.clone(), tickers, receiver)
        }));

        Ok(result)
    }

    pub fn stop(&mut self) -> Result<(), ErrType> {

        self.stop.store(true, Ordering::Release);
        if let Some(h) = self.join_handle.take() {
            match h.join() {
                Ok(_) => (),
                Err(_) => {
                    log::error!("Ошибка разрыва соединения с клиентом {}", self.addr);
                    return Err(NoAccess(format!("Не удалось завершить работу потока обеспечивающего соединение с {}", self.addr).to_string()));
                }
            }
        }
        Ok(())
    }

    pub fn send(stop: Arc<AtomicBool>, addr: String, tickers: Vec<String>, receiver: Receiver<Arc<Vec<StockQuote>>>) {
        let Ok(socket) = UdpSocket::bind(addr.clone()) else {
            log::error!("Не удалось установить соединение с {}", addr);
            return
        };
        let Ok(_) = socket.set_read_timeout(Some(Duration::from_millis(50))) else {
            log::error!("Не удалось огранить upd соединение c {} по времени", addr);
            return
        };
        let mut ping_time = Instant::now();
        let mut buf = [0u8; 2048];

        loop {
            if stop.load(Ordering::Acquire) {
                log::info!("Закрывем соединение с {}", addr);
                drop(stop);
                break;
            }
            if Instant::now() - ping_time > Duration::from_secs(5) {
                log::warn!("Разрываем соединение с {} потому что не получали ping больше 5 сек", addr);
                // drop(socket);
                // break;
            }

            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(all_stocks) => {
                    let filtered_stocks = all_stocks.iter().filter(|x| tickers.contains(&x.ticker)).collect::<Vec<&StockQuote>>();
                    let Ok(data) = bincode::serialize(&filtered_stocks) else {
                        log::error!("Не удалось сериализовать котировки для отправки");
                        break;
                    };
                    let mut response = DATA_REQUEST.to_vec();
                    response.extend_from_slice(&data);
                    let _ = socket.send_to(&response, &addr);
                }
                Err(RecvTimeoutError::Timeout) => {
                    // Не получили котировки продолжаем цикл
                },
                Err(RecvTimeoutError::Disconnected) => {
                    log::error!("Закрылся канал для получения котировок");
                    break;
                }
            }

            match socket.recv_from(&mut buf) {
                Ok((n, from)) => {
                    let t = String::from_utf8_lossy(&buf[..n]);

                    if from.to_string() == addr && &buf[..PING_REQUEST.len()] == PING_REQUEST {
                        log::info!("Клиент {} прислал PING сообщение", addr);
                        let _ = socket.send_to(PONG_REQUEST, from);
                        ping_time = Instant::now();
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::TimedOut => {
                    // За 50 мсек не получили ping. Если время остается, то на следуещем цикле попробуем еще раз
                }
                Err(_) => {
                    log::error!("Произошла ошибка при получении собщений PING от {}", addr);
                    break;
                }
            }
        }
    }
}