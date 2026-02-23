mod udp_server;

use common_lib::errors::ErrType;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

use crate::udp_server::QuoteReader;
use anyhow::{Result, bail};
use clap::Parser;
use std::path::PathBuf;

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
    udp_port: u16,

    #[arg(long)]
    server_ip: String,

    #[arg(long)]
    server_port: u16,
}

fn read_tickers(file_name: &PathBuf) -> Result<HashSet<String>, ErrType> {
    let mut tickers = HashSet::new();
    let file = File::open(file_name).map_err(|e| {
        ErrType::ReadError(format!(
            "Ошибка при открытии файла {}. {}",
            file_name.display(),
            e.to_string()
        ))
    })?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line.map_err(|e| {
            ErrType::ReadError(format!(
                "Ошибка при чтении файла {}. {}",
                file_name.display(),
                e.to_string()
            ))
        })?;
        tickers.insert(line);
    }
    Ok(tickers)
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
        bail!("IP адрес сервера должне содерать только числа и .");
    }

    let server = format!("{}:{}", &cli.server_ip, cli.server_port);
    let Ok(stream) = &mut TcpStream::connect(server.clone()) else {
        bail!("Не удалось установить соединение с {}", server);
    };

    let Ok(_) = stream.set_read_timeout(Some(Duration::from_secs(5))) else {
        bail!(
            "Не удалось установить ограничение по времени для соединения с {}",
            server
        )
    };
    let Ok(cloned_stream) = stream.try_clone() else {
        bail!("Не удалось создать буффер для чтения данных их {}", server)
    };

    let mut reader = BufReader::new(cloned_stream);
    let mut line = String::new();
    let Ok(_) = reader.read_line(&mut line) else {
        bail!("Не удалось прочитать приветственное сообщение сервера")
    };

    let request = format!("{} udp://127.0.0.1:{} {}\n", common_lib::STREAM_REQUEST, cli.udp_port, tickers_join);
    let Ok(_) = stream.write_all(request.as_bytes()) else {
        bail!("Не удалось отправить сообщение {} серверу", request)
    };
    stream.flush()?;
    line.clear();
    let Ok(_) = reader.read_line(&mut line) else {
        bail!("Не удалось прочитать ответ от сервера {}", server)
    };

    if line != "OK\n" {
        bail!(
            "В ответ на сообщение {} сервер прислал ответ {}. Ожидалось OK",
            request,
            line
        );
    }

    let stoper = match common_lib::ctrlc::ctrlc_handler() {
        Ok(stoper) => stoper,
        Err(e) => bail!(e.to_string()),
    };
    env_logger::init();

    match QuoteReader::new(cli.server_ip, cli.server_port, tickers, stoper) {
        Ok(mut reader) => {
            match reader.ping_sender() {
                Ok(_) => {}
                Err(e) => {
                    bail!(e.to_string());
                }
            }
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

    Ok(())
}
