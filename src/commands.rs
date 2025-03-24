use std::{
    fs::File,
    io::{BufRead, Read, Write},
    path::PathBuf,
};

use base64::{prelude::BASE64_STANDARD, Engine};
use regex::Regex;
use serde_json::{json, Value};

const NAME: &str = "tridactyl";
const CONFIG: &str = "tridactylrc";
const VERSION: &str = "0.5.0";

const SUCCESS_CODE: u8 = 0;

fn sanitize_file_name(file_name: &str) -> String {
    let mut result = String::new();
    for c in file_name.to_lowercase().chars() {
        if c.is_alphanumeric() || c == '.' {
            result.push(c);
        }
    }

    result.replace("..", ".")
}

fn expand_tilde(path: String) -> PathBuf {
    if path.starts_with('~') {
        let home = dirs::home_dir().unwrap();
        return home.join(&path[1..path.len()]);
    }

    return PathBuf::from(path);
}

fn expand_vars(path: &str) -> String {
    if !path.contains('$') {
        return path.to_string();
    }

    let mut result = path.to_string();

    if cfg!(unix) {
        let re = Regex::new(r"\$(\w+|\{[^}]*\})").unwrap();
        let mut last_end = 0;
        let mut expanded = String::new();

        for cap in re.captures_iter(path) {
            let whole_match = cap.get(0).unwrap();
            let var_name = cap.get(1).unwrap().as_str();

            expanded.push_str(&path[last_end..whole_match.start()]);

            let clean_name = if var_name.starts_with('{') && var_name.ends_with('}') {
                &var_name[1..var_name.len() - 1]
            } else {
                var_name
            };

            if let Ok(value) = std::env::var(clean_name) {
                expanded.push_str(&value);
            } else {
                expanded.push_str(whole_match.as_str());
            }

            last_end = whole_match.end();
        }

        expanded.push_str(&path[last_end..]);
        result = expanded;
    }

    result
}

fn get_config_file() -> Option<PathBuf> {
    let candidates = [
        dirs::config_dir().unwrap().join(NAME).join(CONFIG),
        dirs::home_dir().unwrap().join(format!(".{}", CONFIG)),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            return Some(candidate.clone());
        }
    }

    None
}

pub fn version() -> Value {
    json!({
        "cmd": "version",
        "code": SUCCESS_CODE,
        "version": VERSION
    })
}

pub fn get_config() -> Value {
    let path = get_config_file();
    if let Some(path) = path {
        let content = std::fs::read_to_string(path);
        if content.is_err() {
            json!({
                "cmd": "getconfig",
                "code": 2
            })
        } else {
            json!({
                "cmd": "getconfig",
                "code": SUCCESS_CODE,
                "content": content.unwrap()
            })
        }
    } else {
        json!({
            "cmd": "getconfig",
            "code": 1
        })
    }
}

pub fn get_config_path() -> Value {
    let path = get_config_file();
    if let Some(path) = path {
        let binding = path.canonicalize().unwrap();
        let path = binding.to_str().unwrap();

        json!({
            "cmd": "getconfigpath",
            "code": SUCCESS_CODE,
            "content": path
        })
    } else {
        json!({
            "cmd": "getconfigpath",
            "code": 1
        })
    }
}

pub(crate) fn read(path: &str) -> Value {
    let path = expand_tilde(expand_vars(path));

    let mut code = SUCCESS_CODE;
    let result = match std::fs::read_to_string(&path) {
        Ok(value) => value,
        Err(_) => {
            code = 2;
            "".into()
        }
    };

    info!(
        "(commands::write) path: {}, code: {}",
        path.to_string_lossy().to_string(),
        code
    );

    json!({
        "cmd": "read",
        "code": code,
        "content": result
    })
}

pub(crate) fn write(path: &str, content: &str) -> Value {
    let re = Regex::new(r"^data:((.*?)(;charset=.*?)?)(;base64)?,").unwrap();

    let mut content = String::from(content);
    if re.is_match(&content) {
        let binding = re.replace(&content, "").to_string();
        let binding =
            String::from_utf8(BASE64_STANDARD.decode(&binding.as_str()).unwrap()).unwrap();
        content = binding;
    }

    let mut code = 2;
    match File::create(path) {
        Ok(mut file) => match file.write_all(content.as_bytes()) {
            Ok(_) => {
                code = SUCCESS_CODE;
            }
            _ => {}
        },
        _ => {}
    }

    info!("(commands::write) path: {}, code: {}", path, code);

    json!({
        "cmd": "write",
        "code": code
    })
}

pub(crate) fn write_rc(path: &str, content: &str, force: bool) -> Value {
    let path = expand_tilde(expand_vars(path));

    let mut code = 1;
    if !std::fs::exists(&path).unwrap_or(false) || force {
        let file = File::create(&path);
        if let Ok(mut file) = file {
            let result = file.write_all(content.as_bytes());
            code = if result.is_ok() { SUCCESS_CODE } else { 2 };
        } else {
            code = 2;
        }
    }

    info!(
        "(commands::write_rc) path: {}, force: {}, code: {}",
        path.to_string_lossy().to_string(),
        force,
        code
    );

    json!({
        "cmd": "writerc",
        "code": code
    })
}

pub(crate) fn create_directory(path: &str) -> Value {
    let path = expand_tilde(expand_vars(path));

    if std::fs::create_dir_all(&path).is_err() {
        return json!({
            "cmd": "mkdir",
            "code": 2
        });
    };

    info!("(commands::mkdir) path: {}", path.to_str().unwrap());
    json!({
        "cmd": "mkdir",
        "code": SUCCESS_CODE,
    })
}

pub(crate) fn read_directory(path: &str) -> Value {
    let mut path = expand_tilde(path.into());

    let is_directory = path.is_dir();
    if !path.is_dir() {
        path = path.parent().unwrap_or(&PathBuf::from(".")).into();
    }

    let mut files = Vec::new();
    if let Ok(entries) = path.read_dir() {
        for entry in entries {
            if let Ok(entry) = entry {
                if let Some(file_name) = entry.path().file_name() {
                    files.push(file_name.to_string_lossy().to_string());
                }
            }
        }
    }

    info!("(commands::temp) path: {}", path.to_string_lossy());
    json!({
        "cmd": "list_dir",
        "isDir": is_directory,
        "files": files,
        "sep": std::path::MAIN_SEPARATOR.to_string()
    })
}

pub(crate) fn temp(prefix: &str, content: &str) -> Option<Value> {
    let prefix = format!("tmp_{}_", sanitize_file_name(prefix));

    let file = tempfile::Builder::new()
        .prefix(&prefix)
        .suffix(".txt")
        .tempfile()
        .ok();

    let Some(mut file) = file else { return None };

    file.write(content.as_bytes()).ok();
    let file_path = file.path().to_str().unwrap_or("");

    info!("(commands::temp) path: {}", file_path);
    Some(json!({
        "cmd": "temp",
        "code": SUCCESS_CODE,
        "content": file_path
    }))
}

pub(crate) fn move_file(from: &str, to: &str, overwrite: bool, cleanup: bool) -> Value {
    let from = expand_tilde(expand_vars(from));
    let to = expand_tilde(expand_vars(to));

    let can_move = overwrite
        || !std::fs::exists(&to).unwrap_or(false)
        || std::fs::exists(to.join(from.file_name().unwrap())).unwrap_or(false);

    let mut code = 1;
    if can_move {
        let result = if std::fs::exists(&to).unwrap_or(false) {
            std::fs::rename(&from, to.join(from.file_name().unwrap()))
        } else {
            std::fs::rename(&from, &to)
        };

        code = if result.is_ok() { SUCCESS_CODE } else { 2 };
    }

    if cleanup {
        std::fs::remove_file(from).unwrap();
    }

    json!({
        "cmd": "move",
        "code": code
    })
}

pub(crate) fn env(key: &str) -> Value {
    match std::env::var(key) {
        Ok(value) => {
            info!("(commands::env) Retrived environment key: {}", key);
            json!({
                "cmd": "env",
                "content": value
            })
        }
        Err(_) => {
            error!("(commands::env) Failed to retrive environment key: {}", key);
            json!({
                "cmd": "env"
            })
        }
    }
}

pub(crate) fn get_process_id() -> Value {
    let pid = std::process::id();
    info!("(commands::get_process_id) Process id: {}", pid);

    json!({
        "cmd": "ppid",
        "content": pid
    })
}

pub(crate) fn run(command: &str, content: Option<&str>) -> Value {
    let mut code = SUCCESS_CODE;
    let mut response = String::new();

    let result = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .spawn();

    if result.is_ok() {
        info!("(commands::run) Ran process: '{}', successfully", command)
    } else {
        error!(
            "(commands::run) Failed to run process: '{}', error: {}",
            command,
            result.as_ref().err().unwrap().to_string()
        )
    }

    if let Ok(mut child) = result {
        if let Some(content) = content {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write(content.as_bytes());
                let _ = stdin.flush();
            }
        }

        if let Some(mut stdout) = child.stdout.take() {
            let mut buffer = vec![];
            let _ = stdout.read_to_end(&mut buffer);

            for line in buffer.lines() {
                response.push_str(format!("{}\n", line.unwrap()).as_str());
            }
        }

        if let Ok(status) = child.wait() {
            code = status.code().unwrap_or(code as i32) as u8;
        }
    };

    json!({
        "cmd": "run",
        "code": code,
        "result": response
    })
}

pub(crate) fn run_async(command: &str) -> Value {
    let mut arguments = command.split_whitespace();

    let result = std::process::Command::new(arguments.next().unwrap())
        .args(arguments)
        .spawn();

    if result.is_ok() {
        info!(
            "(commands::run_async) Ran process: '{}', successfully",
            command
        )
    } else {
        error!(
            "(commands::run_async) Failed to run process: '{}', error: {}",
            command,
            result.err().unwrap().to_string()
        )
    }

    json!({
        "cmd": "run_async"
    })
}
