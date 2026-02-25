use crate::stock_quotes_handler::QuoteHandler;
use crate::udp_server_writer::ServerWriter;
use common_lib::STREAM_REQUEST;
use common_lib::errors::ErrType;
use common_lib::errors::ErrType::ConnectionError;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

///
/// Метод в котором обрабатываем tcp соединение, проверяем данные запросов и создаем upd соединение если все успешно
///
/// # Arguments
///
/// * `stream`: tcp соединение
/// * `stocks`: Хранитель котировок. Нужен для создания канала
///
/// returns: Result<ServerWriter, ErrType>
///
pub fn handle_client(
    stream: TcpStream,
    stocks: &mut QuoteHandler,
) -> Result<ServerWriter, ErrType> {
    let mut writer = match stream.try_clone() {
        Ok(stream) => stream,
        Err(_) => {
            return Err(ConnectionError(
                "Ошибка записи в поток нового tcp соединения".to_string(),
            ));
        }
    };

    let mut write = |text: &str| {
        let _ = writer.write_all(text.as_bytes());
        let _ = writer.flush();
    };

    let mut reader = BufReader::new(stream);
    let sender: ServerWriter;
    let mut line = String::new();

    write("Вы подключились к бирже!\n"); // Приветственное сообщение, для работы не нужно

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

                // Разбиваем запрос по пробелам и проверяем каждую часть отдельно
                let mut parts = input.split_whitespace();
                match parts.next() {
                    Some(STREAM_REQUEST) => {
                        if let Some(host_ports) = parts.next() {
                            let host_parts = host_ports.split(':').collect::<Vec<&str>>();

                            if host_parts[0] != "udp" {
                                log::warn!(
                                    "В принятом запросе {input} отсутствует тип соединения udp"
                                );
                                write("ERROR: Не передан тип соединения udp\n");
                                continue;
                            }

                            let host = host_parts[1][2..].to_string();
                            let port = host_parts[2].to_string();

                            if host.is_empty() || port.is_empty() {
                                log::warn!("В принятом запросе {input} отсутствует адрес и порт");
                                write("ERROR: Не передан адрес и порт\n");
                                continue;
                            }

                            if let Some(tickers) = parts.next() {
                                // Список котировок
                                let tickers_vec = tickers
                                    .split(',')
                                    .map(|x| x.to_string())
                                    .collect::<Vec<String>>();
                                if tickers_vec.is_empty() {
                                    log::warn!(
                                        "В принятом запросе {input} отсутствует список котировок"
                                    );
                                    write("ERROR: Не передан список котировок\n");
                                    continue;
                                } else {
                                    // Отвечаем что все ок что бы клиент запуска udp. И сами тоже создаем udp сокет
                                    log::debug!("Пришел корректный запрос {input}");
                                    write(common_lib::OK_REQUEST);
                                    let address = format!("{host}:{port}");

                                    let Some(receiver) = stocks.create_channel(&address) else {
                                        write(
                                            "ERROR: Произошла ошибка сервера при создании канала свзи",
                                        );
                                        return Err(ErrType::NoAccess(
                                            "Не удалось создать канал для передачи котировок"
                                                .to_string(),
                                        ));
                                    };
                                    sender = ServerWriter::start(address, tickers_vec, receiver)?;
                                    break;
                                }
                            } else {
                                log::warn!(
                                    "В принятом запросе {input} отсутствует список котировок"
                                );
                                write("ERROR: Не передан адрес для udp соединения\n");
                                continue;
                            }
                        } else {
                            log::warn!("В принятом запросе {input} отсутствует upd адрес");
                            write("ERROR: Не передан адрес для udp соединения\n");
                            continue;
                        }
                    }
                    _ => {
                        log::warn!("Получена неизвестная команда {input}");
                        write("ERROR: Получена неизвестная команда\n")
                    }
                };
            }
            Err(e) => {
                log::error!("Произошла ошибка в соединение {:?}", e);
                return Err(ConnectionError(format!(
                    "Произошла ошибка в соединении {:?}",
                    e
                )));
            }
        }
    }
    Ok(sender)
}
