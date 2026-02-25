mod stock_quotes_handler;
mod tcp_server;
mod udp_server_writer;

use crate::stock_quotes_handler::QuoteHandler;
use crate::udp_server_writer::ServerWriter;
use common_lib::TCP_CONNECTION_WAIT_PERIOD;
use common_lib::errors::ErrType;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, ErrorKind};
use std::net::TcpListener;
use std::sync::atomic::Ordering;
use std::{env, io, thread};

fn read_tickers(filename: &str) -> Result<HashSet<String>, ErrType> {
    let mut tickers = HashSet::new();
    let file = File::open(filename)
        .map_err(|e| ErrType::ReadError(format!("Ошибка при открытии файла {filename}. {e}")))?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line
            .map_err(|e| ErrType::ReadError(format!("Ошибка при чтении файла {filename}. {e}")))?;
        tickers.insert(line);
    }
    Ok(tickers)
}

fn main() -> io::Result<()> {
    env_logger::init();

    let listener = TcpListener::bind("127.0.0.1:1111")?;
    listener.set_nonblocking(true)?;
    log::info!("TCP сервер начал работу и слушает порт 1111");

    let args: Vec<String> = env::args().collect();

    let filename = if args.len() < 2 {
        "tickers.txt"
    } else {
        args[1].as_str()
    };

    let tickers = match read_tickers(filename) {
        Ok(tickers) => tickers,
        Err(e) => {
            log::error!("Не удалось прочитать список котировок из файла {filename}",);
            return Err(e.into());
        }
    };

    let stoper = common_lib::ctrlc::ctrlc_handler()?;

    let mut stocks = QuoteHandler::new(&tickers);

    let mut senders: Vec<ServerWriter> = Vec::new();

    // Ловим новые tcp соединения и каждое соединение обрабатываем в методе handle_client
    for stream in listener.incoming() {
        if stoper.load(Ordering::Acquire) {
            log::info!("Остановка работы tcp сервера");
            break;
        }
        senders
            .iter()
            .filter(|s| s.stop.load(Ordering::Acquire))
            .for_each(|s| stocks.remove_channel(&s.remote_address));
        senders.retain(|s| !s.stop.load(Ordering::Acquire));

        match stream {
            Ok(stream) => {
                // Поскольку обработка соединение не долгая все делается в одном потоке
                match tcp_server::handle_client(stream, &mut stocks) {
                    Ok(sender) => {
                        // Сохраняем соединение, что бы при остановке сервера корректно их закрыть
                        senders.push(sender);
                    }
                    Err(e) => {
                        log::error!("Не удалось установить соединение. Ошибка {e}");
                        continue;
                    }
                }
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                // Если нет соединение тогда спать. Нужно, что бы отлавливать ctrlc команды.
                thread::sleep(TCP_CONNECTION_WAIT_PERIOD);
            }
            Err(e) => eprintln!("Connection failed: {e}"),
        }
    }

    for sender in &mut senders {
        match sender.stop() {
            Ok(_) => (),
            Err(e) => {
                log::error!("{e}");
            }
        }
    }
    stocks.stop()?;
    Ok(())
}
