use std::io::{BufRead, Write, BufReader};
use std::net::TcpStream;
use common_lib::STREAM_REQUEST;
use common_lib::errors::ErrType;
use common_lib::errors::ErrType::ConnectionError;
use crate::udp_monitoring::UdpSender;

pub fn handle_client(stream: TcpStream) -> Result<UdpSender, ErrType>{
    let mut writer = match stream.try_clone() {
        Ok(stream) => stream,
        Err(_) => {
            return Err(ConnectionError("Ошибка записи в поток нового tcp соединения".to_string()));
        }
    };

    let mut write = |text: &str| {
        let _ = writer.write_all(text.as_bytes());
        let _ = writer.flush();
    };
    let mut reader = BufReader::new(stream);
    let sender: UdpSender;
    let mut line = String::new();

    write("Вы подключились к бирже!\n");

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                return Err(ErrType::RequestError("Пришел пустой запрос".to_string()));
            }
            Ok(_) => {
                let input = line.trim();
                if input.is_empty() {
                    return Err(ErrType::RequestError("Пришел пустой запрос".to_string()));
                }

                let mut parts = input.split_whitespace();
                match parts.next() {
                    Some(STREAM_REQUEST) => {
                        if let Some(host_ports) = parts.next() {

                            let host_parts = host_ports.split(':').collect::<Vec<&str>>();

                            if host_parts[0] != "udp" {
                                log::warn!("В принятом запросе {} отсутствует тип соединения udp", input);
                                write("ERROR: Не передан тип соединения udp\n");
                                continue;
                            }

                            let host = host_parts[1][2..].to_string();
                            let port = host_parts[2].to_string();

                            if host.is_empty() || port.is_empty() {
                                log::warn!("В принятом запросе {} отсутствует адрес и порт", input);
                                write("ERROR: Не передан адрес и порт\n");
                                continue;
                            }

                            let address = format!("{}:{}", host, port);

                            if let Some(tickers) = parts.next()
                            {
                                let tickers_vec = tickers.split(',').map(|x| x.to_string()).collect::<Vec<String>>();
                                if tickers_vec.is_empty() {
                                    log::warn!("В принятом запросе {} отсутствует список котировок", input);
                                    write("ERROR: Не передан список котировок\n");
                                    continue;
                                } else {
                                    log::debug!("Пришел корректный запрос {}", input);
                                    write("OK\n");
                                    sender = UdpSender::start(address, tickers_vec)?;
                                    break;
                                }
                            } else {
                                log::warn!("В принятом запросе {} отсутствует список котировок", input);
                                write("ERROR: Не передан адрес для udp соединения\n");
                                continue;
                            }

                        } else {
                            log::warn!("В принятом запросе {} отсутствуе upd адрес", input);
                            write("ERROR: Не передан адрес для udp соединения\n");
                            continue;
                        }
                    }
                    _ => {
                        log::warn!("Получина неизвестная команда {}", input);
                        write("ERROR: unknown command\n")
                    },
                };
            }
            Err(e) => {
                log::error!("Произошла ошибка в соединениее {:?}", e);
                return Err(ConnectionError(format!("Произошла ошибка в соединении {:?}", e)));
            }
        }
    }
    Ok(sender)
}
