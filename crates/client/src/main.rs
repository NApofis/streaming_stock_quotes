mod udp_client_reader;

use common_lib::errors::ErrType;
use common_lib::{OK_REQUEST, STREAM_REQUEST, TCP_CONNECTION_WAIT_PERIOD};
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

use crate::udp_client_reader::ClientReader;
use anyhow::{Result, bail};
use clap::Parser;
use std::path::PathBuf;

use fern::Dispatch;
use log::{Level, LevelFilter};
use std::fs::OpenOptions;

#[derive(Debug, Parser)]
#[command(
    name = "quote_client",
    version,
    about = "Приложение для получение котировок"
)]
struct Cli {
    #[arg(long)]
    tickers_file: PathBuf,

    #[arg(long)]
    client_ip: String,

    #[arg(long)]
    client_port: u16,

    #[arg(long)]
    server_ip: String,

    #[arg(long)]
    server_port: u16,
}

fn read_tickers(file_name: &PathBuf) -> Result<HashSet<String>, ErrType> {
    let mut tickers = HashSet::new();
    let file = File::open(file_name).map_err(|e| {
        ErrType::ReadError(format!(
            "Ошибка при открытии файла {}. {e}",
            file_name.display()
        ))
    })?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line.map_err(|e| {
            ErrType::ReadError(format!(
                "Ошибка при чтении файла {}. {e}",
                file_name.display()
            ))
        })?;
        tickers.insert(line);
    }
    Ok(tickers)
}

fn setup_logger(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    // Общий формат
    let base_config = Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}][{}][{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.target(),
                message
            ))
        })
        .level(LevelFilter::Debug); // максимально возможный уровень

    let file_log_name = format!("client_{}.log", port);

    let file_logger = Dispatch::new()
        .level(LevelFilter::Debug)
        .chain(
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true) // очистим старые логи при старте
                .open(file_log_name)?,
        );

    // В консоль попадает только Error все остальное в файл
    let console_logger = Dispatch::new()
        .filter(|metadata| metadata.level() == Level::Error)
        .chain(std::io::stderr());

    base_config
        .chain(file_logger)
        .chain(console_logger)
        .apply()?;

    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let tickers = match read_tickers(&cli.tickers_file) {
        Ok(tickers) => tickers,
        Err(e) => {
            bail!(e.to_string());
        }
    };
    let tickers_join = tickers
        .iter()
        .map(String::as_str)
        .collect::<Vec<&str>>()
        .join(",");

    if !cli
        .server_ip
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.')
    {
        bail!("IP адрес сервера должен содержать только числа и .");
    }

    let server = format!("{}:{}", &cli.server_ip, cli.server_port);
    let Ok(stream) = &mut TcpStream::connect(server.clone()) else {
        bail!("Не удалось установить соединение с {server}");
    };

    let Ok(_) = stream.set_read_timeout(Some(TCP_CONNECTION_WAIT_PERIOD)) else {
        bail!("Не удалось установить ограничение по времени для соединения с {server}")
    };
    let Ok(cloned_stream) = stream.try_clone() else {
        bail!("Не удалось создать буфер для чтения данных их {}", server)
    };

    let mut reader = BufReader::new(cloned_stream);
    let mut line = String::new();
    // Тут должны получить приветственное сообщение
    let Ok(_) = reader.read_line(&mut line) else {
        bail!("Не удалось прочитать приветственное сообщение сервера")
    };

    // Отправляем сообщение, что бы начать получать котировки
    let address_udp = format!("{}:{}", &cli.client_ip, &cli.client_port);
    let request = format!("{STREAM_REQUEST} udp://{address_udp} {tickers_join}\n");
    let Ok(_) = stream.write_all(request.as_bytes()) else {
        bail!("Не удалось отправить сообщение {request} серверу")
    };
    stream.flush()?;
    line.clear();
    let Ok(_) = reader.read_line(&mut line) else {
        bail!("Не удалось прочитать ответ от сервера {server}")
    };

    // Если серверу все понравилось тогда запускаем udp соединение
    if line != OK_REQUEST {
        bail!("В ответ на сообщение {request} сервер прислал ответ {line}. Ожидалось OK");
    }

    let stoper = match common_lib::ctrlc::ctrlc_handler() {
        Ok(stoper) => stoper,
        Err(e) => bail!(e.to_string()),
    };

    let Ok(_) = setup_logger(cli.client_port) else {
        bail!("Не удалось запустить логер")
    };

    let thread_handler;
    match ClientReader::new(address_udp, cli.server_ip, tickers, stoper) {
        Ok(mut reader) => {
            // Запускаем ping
            match reader.ping_sender() {
                Ok(jh) => {
                    thread_handler = jh;
                }
                Err(e) => {
                    bail!(e.to_string());
                }
            }
            // Запускаем udp соединение
            match reader.start() {
                Ok(_) => {}
                Err(e) => {
                    bail!(e.to_string());
                }
            }
        }
        Err(e) => {
            bail!(e.to_string());
        }
    };

    // Завершаем работу потока для отправки ping запроса
    match thread_handler.join() {
        Ok(_) => {}
        Err(_) => {
            bail!("Не удалось остановить работу потока отсылающего ping сообщения");
        }
    }
    Ok(())
}
