#[macro_use]
extern crate log;
extern crate simplelog;

pub mod commands;

use dirs;
use std::{
    fs::File,
    io::{Read, Stdin, Stdout, Write},
};

use serde_json::{json, Value};
use simplelog::{Config, LevelFilter, WriteLogger};

const NATIVE_MESSAGE_HOST: &str = "tridactyl.json";
const BROWSERS: [&str; 2] = [".mozilla", ".librewolf"];

fn handle_command(command: &Value) -> Value {
    let error = json!({
        "cmd": "error",
        "code": 1,
        "error": "Unhandled message"
    });

    // TODO: kill this nest
    let response = match command {
        Value::Object(map) => match map.get("cmd") {
            Some(name) => {
                let command_name = match name {
                    Value::String(name) => name.as_str(),
                    _ => "",
                };

                match command_name {
                    "env" => {
                        let Some(key) = map.get("var") else {
                            return error;
                        };

                        let Some(key) = key.as_str() else {
                            return error;
                        };

                        commands::env(key)
                    }

                    "version" => commands::version(),

                    "getconfig" => commands::get_config(),
                    "getconfigpath" => commands::get_config_path(),

                    "read" => {
                        let path = map.get("file").and_then(|v| v.as_str()).unwrap_or_default();
                        commands::read(path)
                    }

                    "write" => {
                        let path = map.get("file").and_then(|v| v.as_str()).unwrap_or_default();
                        let content = map
                            .get("content")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default();

                        commands::write(path, content)
                    }

                    "writerc" => {
                        let path = map.get("file").and_then(|v| v.as_str()).unwrap_or_default();
                        let force = map.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
                        let content = map
                            .get("content")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default();

                        commands::write_rc(path, content, force)
                    }

                    "move" => {
                        let from = map.get("from").and_then(|v| v.as_str()).unwrap_or_default();
                        let to = map.get("to").and_then(|v| v.as_str()).unwrap_or_default();

                        let overwrite = map
                            .get("overwrite")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);

                        let cleanup = map
                            .get("cleanup")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);

                        commands::move_file(from, to, overwrite, cleanup)
                    }

                    "mkdir" => {
                        let path = map.get("dir").and_then(|v| v.as_str()).unwrap_or_default();
                        commands::create_directory(path)
                    }

                    "list_dir" => {
                        let path = map.get("path").and_then(|v| v.as_str()).unwrap_or_default();
                        commands::read_directory(path)
                    }

                    "temp" => {
                        let prefix = map
                            .get("prefix")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default();

                        let content = map
                            .get("content")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default();

                        if let Some(result) = commands::temp(prefix, content) {
                            result
                        } else {
                            error
                        }
                    }

                    "run" => {
                        let command = map
                            .get("command")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default();

                        let content = if let Some(value) = map.get("content") {
                            value.as_str()
                        } else {
                            None
                        };

                        commands::run(command, content)
                    }

                    "run_async" => {
                        let command = map
                            .get("command")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default();

                        commands::run_async(command)
                    }

                    "ppid" => commands::get_process_id(),

                    _ => error,
                }
            }

            _ => error,
        },

        _ => error,
    };

    response
}

fn get_message(stream: &mut Stdin) -> Option<Value> {
    let mut buffer = [0u8; 4];
    stream.read_exact(&mut buffer).unwrap();

    let length = u32::from_ne_bytes(buffer);
    if length == 0 {
        return None;
    }

    debug!("Received message from client with length of {}", length);

    let mut buffer = vec![0u8; length as usize];
    stream.read_exact(&mut buffer).unwrap();

    let string = String::from_utf8(buffer).unwrap();
    let json: Value = serde_json::from_str(string.as_str()).unwrap();

    Some(json)
}

fn send_message(stream: &mut Stdout, json: &Value) {
    let mut handle = stream.lock();
    let response = &handle_command(&json).to_string();

    info!("Sending message to client");

    handle
        .write(&(response.len() as u32).to_ne_bytes())
        .unwrap();
    handle.write(&response.as_bytes()).unwrap();
    handle.flush().unwrap();
}

fn main() {
    let log_path = dirs::data_dir().unwrap().join("tridactyl");
    std::fs::create_dir_all(&log_path).unwrap();

    let log_file = File::options()
        .append(true)
        .create(true)
        .open(log_path.join("tridactyl.log"))
        .unwrap();

    WriteLogger::init(LevelFilter::Info, Config::default(), log_file).unwrap();

    debug!("Ran the tridactyl native executable");

    let arguments = std::env::args().collect::<Vec<_>>();
    if let Some(argument) = arguments.get(1) {
        match argument.as_str() {
            "-h" => return usage(),
            "--help" => return usage(),
            "--setup" => return setup_tridactyl(),

            _ => {}
        }
    }

    let mut stream = std::io::stdin();
    let mut stream_out = std::io::stdout();

    loop {
        let json = get_message(&mut stream);
        if let Some(json) = json {
            send_message(&mut stream_out, &json);
        };
    }
}

fn usage() {
    println!(env!("CARGO_PKG_DESCRIPTION"));
    println!("Usage: tridactyl-native [options]");
    println!("\nOptions:");
    println!("\t-h, --help\tDisplay this message");
    println!("\t--setup   \tSetup tridactyl");
}

fn setup_tridactyl() {
    let home = dirs::home_dir().unwrap();
    for browser in BROWSERS {
        let path = home.join(browser);
        if path.exists() {
            let path = path.join("native-messaging-hosts");
            std::fs::create_dir_all(&path).unwrap();

            let path = path.join(NATIVE_MESSAGE_HOST);
            let content = format!(
                include_str!("../tridactyl.json"),
                std::env::current_exe().unwrap().to_str().unwrap()
            );

            println!("installing manifest to: {}", path.to_str().unwrap());
            std::fs::write(path, content).unwrap();
        }
    }
}
