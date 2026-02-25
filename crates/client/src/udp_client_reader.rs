use chrono::{DateTime, Utc};
use common_lib::errors::ErrType;
use common_lib::stock_quote::StockQuote;
use common_lib::{DATA_REQUEST, PING_REQUEST, PONG_REQUEST, QUOTE_GENERATOR_PERIOD, QUOTES_WAIT_PERIOD, PING_SEND_PERIOD, MAX_NUMBER_IGNORED_PING};
use std::collections::HashSet;
use std::io;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::Instant;

struct ServerInfo {
    ip: String,
    socket: Arc<Mutex<String>>,
    is_set: Arc<AtomicBool>,
}

pub struct ClientReader {
    socket: UdpSocket,
    tickers: HashSet<String>,
    local_address: String,
    stoper: Arc<AtomicBool>,
    expect_pong: Arc<AtomicBool>,
    remote_server_info: ServerInfo,
}

impl ClientReader {
    ///
    /// Создает сокет для отправки запросов по upd на сервер
    ///
    /// # Arguments
    ///
    /// * `address`: адрес для сокета
    /// * `server_ip`: ip адрес сервера. Порта нет так как сервер создает новый сокет и пока не знает какой порт ему выдадут
    /// * `tickers`: список котировок который клиент запрашивает у сервера. Нужно для проверки.
    /// * `stop`: атомик по которому завершает работу клиент
    ///
    /// returns: Result<ClientReader, ErrType>
    ///
    pub fn new(
        address: String,
        server_ip: String,
        tickers: HashSet<String>,
        stop: Arc<AtomicBool>,
    ) -> Result<Self, ErrType> {
        // Сокет создаем на адресе который отправили серверу
        let socket = match UdpSocket::bind(address.clone()) {
            Ok(socket) => socket,
            Err(e) => {
                log::error!("Не удалось запустить udp сервер на сокете {address}. {e}");
                return Err(ErrType::ConnectionError(
                    "Не удалось запустить upd сервер".to_string(),
                ));
            }
        };

        let Ok(_) = socket.set_nonblocking(true) else {
            log::error!(
                "Не удалось сделать upd сокет c {} не блокирующимся",
                address.clone()
            );
            return Err(ErrType::ConnectionError(format!(
                "Не удалось сделать upd сокет c {address} не блокирующимся"
            )));
        };

        Ok(Self {
            socket,
            tickers,
            local_address: address,
            stoper: stop,
            expect_pong: Arc::new(AtomicBool::new(false)),
            remote_server_info: ServerInfo {
                ip: server_ip,
                socket: Arc::new(Mutex::new("".to_string())),
                is_set: Arc::new(AtomicBool::new(false)),
            },
        })
    }

    ///
    /// Метод для в котором крутится цикл и проверяет udp запросы. Без отдельного потомка потому что именно этот цикл обеспечивает непрерывную работу клиента
    ///
    pub fn start(&mut self) -> Result<(), ErrType> {
        let mut buf = [0u8; 2048];

        let mut deadline = Instant::now() + QUOTES_WAIT_PERIOD;

        loop {
            // Если установили флаг завершения работы
            if self.stoper.load(Ordering::Acquire) {
                log::info!("Закрываем соединение");
                return Ok(());
            }

            match self.socket.recv_from(&mut buf) {
                Ok((n, from)) => {
                    // Поскольку порт udp сокета сервера не известен ждем запрос с ip сервера, далее получаем его порт.
                    // Все запросы с других ip игнорируем
                    if from.ip().to_string() != self.remote_server_info.ip {
                        log::error!(
                            "Пришел запрос от неизвестной машины {from}: {}",
                            String::from_utf8_lossy(&buf[..n])
                        );
                        continue;
                    }

                    if !self.remote_server_info.is_set.load(Ordering::Acquire) {
                        if let Ok(mut s) = self.remote_server_info.socket.lock() {
                            // фиксируем хост и порт сервера и помечаем что бы началась отправка ping сообщений
                            *s = from.to_string();
                            self.remote_server_info
                                .is_set
                                .store(true, Ordering::Release);
                        } else {
                            log::error!(
                                "Не удалось зафиксировать адрес удаленной машины {from}: {}",
                                String::from_utf8_lossy(&buf[..n])
                            );
                            continue;
                        }
                    }

                    // Проверяем полученный запрос и отсеиваем неизвестные
                    if &buf[..DATA_REQUEST.len()] == DATA_REQUEST {
                        log::info!(
                            "От {from} пришли данные {}",
                            String::from_utf8_lossy(&buf[..n])
                        );
                    } else if &buf[..PONG_REQUEST.len()] == PONG_REQUEST {
                        log::info!(
                            "От {from} пришел {} запрос",
                            String::from_utf8_lossy(&buf[..n])
                        );
                        self.expect_pong.store(false, Ordering::Release);
                        continue;
                    } else {
                        log::error!(
                            "От {from} пришел неизвестный запрос {}",
                            String::from_utf8_lossy(&buf[..n])
                        );
                        continue;
                    }

                    deadline = Instant::now() + QUOTES_WAIT_PERIOD;
                    // Если прислали данные, то пробуем их десириализовать и выводим в консоль
                    match bincode::deserialize::<Vec<StockQuote>>(&buf[DATA_REQUEST.len()..n]) {
                        Ok(quotes) => {
                            if self.tickers.len() != quotes.len() {
                                log::error!(
                                    "Сервер вернул не все запрашиваемые значения Запрашивали: {}; Пришло:{}",
                                    self.tickers
                                        .iter()
                                        .map(String::as_str)
                                        .collect::<Vec<_>>()
                                        .join(","),
                                    quotes
                                        .iter()
                                        .map(|x| x.ticker.as_str())
                                        .collect::<Vec<_>>()
                                        .join(",")
                                );
                            }
                            println!("---");
                            for quote in quotes {
                                if !self.tickers.contains(&quote.ticker) {
                                    log::error!(
                                        "Сервер не вернул запрашиваемое значение {}",
                                        quote.ticker
                                    );
                                }
                                let mut date_time_string = String::new();
                                if let Some(dt) =
                                    DateTime::<Utc>::from_timestamp_millis(quote.timestamp)
                                {
                                    date_time_string =
                                        format!("на {} ", dt.format("%Y-%m-%d %H:%M:%S"));
                                } else {
                                    log::error!(
                                        "Сервер вернул неизвестное время {}",
                                        quote.timestamp
                                    );
                                };
                                println!(
                                    "  Акции {} -> {}продано {} по цене {}",
                                    quote.ticker, date_time_string, quote.volume, quote.price
                                );
                            }
                        }
                        Err(e) => {
                            log::error!(
                                "Ошибка десериализации ответа: {e} {}",
                                String::from_utf8_lossy(&buf[..n])
                            );
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // За период не пришли данные котировок
                    if Instant::now() >= deadline {
                        log::warn!(
                            "Сервер {} не прислал котировки за последние {} секунды",
                            self.local_address,
                            QUOTES_WAIT_PERIOD.as_secs()
                        );
                        deadline += QUOTES_WAIT_PERIOD;
                    }
                    thread::sleep(QUOTE_GENERATOR_PERIOD);
                }
                Err(e) => {
                    log::error!(
                        "Произошла ошибка при получении сообщений от сервера {}",
                        self.local_address
                    );
                    return Err(ErrType::RequestError(format!(
                        "Произошла ошибка при получении сообщений от сервера {e}"
                    )));
                }
            }
        }
    }

    ///
    /// Метод в котором запускается поток для отправки ping сообщений
    ///
    /// returns: поток для завершения работы
    ///
    pub fn ping_sender(&self) -> Result<JoinHandle<()>, ErrType> {
        let Ok(copy_socket) = self.socket.try_clone() else {
            Err(ErrType::ConnectionError(
                "Не удалось создать копию сокета для отправки PING сообщений".to_string(),
            ))?
        };

        let local_expect_pong = Arc::clone(&self.expect_pong); // копия атомика через который определяется прислал ли сервер pong на наш ping
        let local_stoper = Arc::clone(&self.stoper); // копия атомика через который происходит завершение работы всего клиента

        let server_address = self.remote_server_info.socket.clone(); // Адрес сервера что бы знать куда отправлять ping
        let server_address_set = self.remote_server_info.is_set.clone(); // Флаг установлен если известен адрес сервсера

        Ok(thread::spawn(move || {
            let mut remote_server_socket = String::new();
            let mut fail = 0;
            loop {

                if local_stoper.load(Ordering::Acquire) {
                    log::info!("Завершаем отправлять ping запросы");
                    break;
                };

                if remote_server_socket.is_empty() {
                    if server_address_set.load(Ordering::Acquire) {
                        if let Ok(s) = server_address.lock() {
                            remote_server_socket = s.clone();
                        } else {
                            log::warn!("Неизвестен адрес удаленный машины что бы отправить PING");
                            continue;
                        }
                    } else {
                        // Пока адреса нет тогда засыпаем и чекам по таймауту
                        thread::sleep(PING_SEND_PERIOD);
                        continue;
                    }
                }


                // Если переменная все еще установлена то значит pong не пришел и можно закрывать работу
                if local_expect_pong.load(Ordering::Acquire) {
                    log::info!(
                        "Сервер {remote_server_socket} не прислал PONG в течении {} секунды.",
                        PING_SEND_PERIOD.as_secs()
                    );
                    fail += 1;
                    if fail >= MAX_NUMBER_IGNORED_PING {
                        log::error!("Сервер {remote_server_socket} не ответил на 3 PING сообщения. Соединение будет закрыто.");
                        local_stoper.store(true, Ordering::Release);
                        continue;
                    }
                } else {
                    fail = 0;
                }

                let Ok(_) = copy_socket.send_to(PING_REQUEST, remote_server_socket.clone()) else {
                    log::error!(
                        "Не удалось отправить PING сообщение на адрес {}",
                        remote_server_socket
                    );
                    break;
                };

                local_expect_pong.store(true, Ordering::Release);
                thread::sleep(PING_SEND_PERIOD);
            }
        }))
    }
}
