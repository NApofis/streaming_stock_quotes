use common_lib::errors::ErrType;
use common_lib::errors::ErrType::NoAccess;
use common_lib::stock_quote::StockQuote;
use common_lib::{
    DATA_REQUEST, PING_WAIT_PERIOD, PING_REQUEST, PONG_REQUEST, UDP_SERVER_RECEIVE_PERIOD,
};
use crossbeam_channel::{Receiver, RecvTimeoutError};
use std::io;
use std::net::UdpSocket;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use std::{thread, thread::JoinHandle};

pub struct ServerWriter {
    pub stop: Arc<AtomicBool>,
    pub remote_address: String,
    join_handle: Option<JoinHandle<()>>,
}

impl ServerWriter {
    ///
    /// Создать поток в котором будет поддерживаться соединение udp соединения для передачи котировок
    ///
    /// # Arguments
    ///
    /// * `addr`: хост порт для подключения. Т.е. адрес клиента
    /// * `tickers`: список котировок которые ожидает клиент
    /// * `receiver`: канал откуда получаем полный список котировок
    ///
    /// returns: Result<ServerWriter, ErrType>
    ///
    pub fn start(
        addr: String,
        tickers: Vec<String>,
        receiver: Receiver<Arc<Vec<StockQuote>>>,
    ) -> Result<Self, ErrType> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();

        let mut result = Self {
            stop,
            remote_address: addr.clone(),
            join_handle: None,
        };

        result.join_handle = Some(thread::spawn(move || {
            Self::send(stop_clone, addr.clone(), tickers, receiver)
        }));

        Ok(result)
    }

    ///
    /// Установить флаг для завершения работы потоков которые поддерживают соединение
    ///
    pub fn stop(&mut self) -> Result<(), ErrType> {
        self.stop.store(true, Ordering::Release);
        if let Some(h) = self.join_handle.take() {
            match h.join() {
                Ok(_) => (),
                Err(_) => {
                    log::error!(
                        "Ошибка разрыва соединения с клиентом {}",
                        self.remote_address
                    );
                    return Err(NoAccess(
                        format!(
                            "Не удалось завершить работу потока обеспечивающего соединение с {}",
                            self.remote_address
                        )
                        .to_string(),
                    ));
                }
            }
        }
        Ok(())
    }

    ///
    /// Метод для потока в котором будет поддерживаться udp соединение с клиентом
    ///
    /// # Arguments
    ///
    /// * `stop`: флаг для плавного завершения работы потока и закрытия соединение
    /// * `addr`: адресс клиента
    /// * `tickers`: список котировок
    /// * `receiver`: канал для получения данных
    ///
    /// returns: ()
    ///
    pub fn send(
        stop: Arc<AtomicBool>,
        addr: String,
        tickers: Vec<String>,
        receiver: Receiver<Arc<Vec<StockQuote>>>,
    ) {
        // Так как для udp у сервера должен быть отдельный сокет, адрес задается дефолтный, что бы ОС выдала свободный порт
        let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else {
            log::error!("Не удалось установить соединение с {addr}");
            return;
        };
        // ограничим ожидание по времени что бы успевать чекнуть новые данные котировок
        let Ok(_) = socket.set_read_timeout(Some(UDP_SERVER_RECEIVE_PERIOD)) else {
            log::error!("Не удалось огранить upd соединение c {addr} по времени");
            return;
        };
        let mut ping_time = Instant::now();
        let mut buf = [0u8; 2048];

        loop {
            // Завершение когда долго не было ping от клиента
            if Instant::now() - ping_time > PING_WAIT_PERIOD {
                log::warn!("Разрываем соединение с {addr} потому что не получали ping больше {} сек", PING_WAIT_PERIOD.as_secs());
                stop.store(true, Ordering::Release);
            }

            // Завершение когда вызывали метод stop
            if stop.load(Ordering::Acquire) {
                log::info!("Закрываем соединение с {addr}");
                break;
            }

            // проверяем нет ли новых данных для котировок
            match receiver.recv_timeout(UDP_SERVER_RECEIVE_PERIOD) {
                Ok(all_stocks) => {
                    // Фильтруем и сериализуем котировки
                    let filtered_stocks = all_stocks
                        .iter()
                        .filter(|x| tickers.contains(&x.ticker))
                        .collect::<Vec<&StockQuote>>();
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
                }
                Err(RecvTimeoutError::Disconnected) => {
                    log::error!("Закрылся канал для получения котировок");
                    break;
                }
            }

            // Проверяем ping от клиента
            match socket.recv_from(&mut buf) {
                Ok((n, from)) => {
                    if from.to_string() == addr && &buf[..PING_REQUEST.len()] == PING_REQUEST {
                        log::info!("Клиент {} прислал PING сообщение", addr);
                        let _ = socket.send_to(PONG_REQUEST, from);
                        ping_time = Instant::now(); // Обновляем время для последнего ping сообщения
                    } else {
                        // Если прислали что-то другое тогда ничего не меняем. Если ping так и не придет, тогда завершимся по таймауту
                        log::warn!(
                            "Получен неизвестный запрос {from}: {}",
                            String::from_utf8_lossy(&buf[..n])
                        )
                    }
                }
                Err(e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        || e.kind() == io::ErrorKind::TimedOut =>
                {
                    // За таймаут не получили ping. Если время остается, то на следующем цикле попробуем еще раз
                }
                Err(_) => {
                    log::error!("Произошла ошибка при получении сообщения PING от {}", addr);
                    break;
                }
            }
        }
    }
}
