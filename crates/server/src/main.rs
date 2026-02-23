mod stock_quotes_handler;
mod tcp_server;
mod udp_monitoring;

use std::collections::{HashSet, LinkedList};
use std::{env, io, thread};
use std::net::TcpListener;
use std::fs::File;
use std::io::{BufRead, BufReader, ErrorKind};
use std::sync::atomic::Ordering;
use std::time::Duration;
use common_lib::errors::ErrType;
use crate::udp_monitoring::UdpSender;

fn read_tickers(file_name: &str) -> Result<HashSet<String>, ErrType> {
    let mut tickers = HashSet::new();
    let file =  File::open(file_name).map_err(|e| ErrType::ReadError(format!("Ошибка при открытии файла {}. {}", file_name.to_string(), e.to_string())))?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line.map_err(|e| ErrType::ReadError(format!("Ошибка при чтении файла {}. {}", file_name.to_string(), e.to_string())))?;
        tickers.insert(line);
    }
    Ok(tickers)
}

fn main() -> io::Result<()> {
    env_logger::init();

    let listener = TcpListener::bind("127.0.0.1:8080")?;
    listener.set_nonblocking(true)?;
    log::info!("TCP сервер начал работу и слушает порт 8080");

    let args: Vec<String> = env::args().collect();
    let filename: &str;
    if args.len() < 2 {
        filename = "tickers.txt";
    }
    else {
        filename = args[1].as_str();
    }

    let tickers = match read_tickers(filename) {
        Ok(tickers) => tickers,
        Err(e) => {
            log::error!("Не удалось прочитать список котировок из файла {}", filename);
            return Err(e.into());
        }
    };

    let stoper = common_lib::ctrlc::ctrlc_handler()?;

    let mut stocks = stock_quotes_handler::QuoteHandler::new(&tickers);

    let mut senders: LinkedList<UdpSender> = LinkedList::new();

    for stream in listener.incoming() {
        if stoper.load(Ordering::Acquire) {
            log::info!("Остановка работы tcp сервера");
            break;
        }
        match stream {
            Ok(stream) => {
                match tcp_server::handle_client(stream) {
                    Ok(sender) => {
                        senders.push_back(sender);
                    },
                    Err(e) => {
                        log::error!("Не удалось установить соединение. Ошибка {}", e);
                        continue;
                    }
                }
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                // Если нет соединение тогда спать. Нужно, что бы отлавливать ctrlc команды.
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => eprintln!("Connection failed: {}", e),
        }
    }

    for sender in &mut senders {
        match sender.stop() {
            Ok(_) => (),
            Err(e) => {
                log::error!("{}", e.to_string());
            }
        }
    }
    stocks.stop()?;
    Ok(())
}
