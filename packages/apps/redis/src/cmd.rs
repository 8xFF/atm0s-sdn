use std::{collections::VecDeque, io::BufReader};

fn get_string(value: resp::Value) -> Option<String> {
    if let resp::Value::Bulk(value) = value {
        Some(value)
    } else {
        None
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RedisCmd {
    Command(Option<String>),
    Get(String),
    Del(String),
    Set(String, String),
    HGet(String),
    HSet(String, String, String),
    HDel(String, String),
    Sub(String),
    HSub(String),
}

impl TryFrom<&[u8]> for RedisCmd {
    type Error = String;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let mut decoder = resp::Decoder::new(BufReader::new(value));
        match decoder.decode().map_err(|e| e.to_string())? {
            resp::Value::Null => todo!(),
            resp::Value::NullArray => todo!(),
            resp::Value::String(_) => todo!(),
            resp::Value::Error(_) => todo!(),
            resp::Value::Integer(_) => todo!(),
            resp::Value::Bulk(_) => todo!(),
            resp::Value::BufBulk(_) => todo!(),
            resp::Value::Array(cmds) => {
                let mut cmds = VecDeque::from(cmds);
                log::debug!("cmds: {:?}", cmds);
                let cmd_type = cmds.pop_front().ok_or("Empty command")?;
                if let resp::Value::Bulk(cmd_type) = cmd_type {
                    match cmd_type.as_str() {
                        "COMMAND" => Ok(RedisCmd::Command(cmds.pop_front().and_then(get_string))),
                        "GET" => Ok(RedisCmd::Get(cmds.pop_front().and_then(get_string).ok_or("Empty key")?)),
                        "DEL" => Ok(RedisCmd::Del(cmds.pop_front().and_then(get_string).ok_or("Empty key")?)),
                        "SET" => {
                            let key = cmds.pop_front().and_then(get_string).ok_or("Empty key")?;
                            let value = cmds.pop_front().and_then(get_string).ok_or("Empty value")?;
                            Ok(RedisCmd::Set(key, value))
                        }
                        "HGET" => Ok(RedisCmd::HGet(cmds.pop_front().and_then(get_string).ok_or("Empty key")?)),
                        "HDEL" => Ok(RedisCmd::HDel(
                            cmds.pop_front().and_then(get_string).ok_or("Empty key")?,
                            cmds.pop_front().and_then(get_string).ok_or("Empty sub_key")?,
                        )),
                        "HSET" => {
                            let key = cmds.pop_front().and_then(get_string).ok_or("Empty key")?;
                            let sub_key = cmds.pop_front().and_then(get_string).ok_or("Empty sub_key")?;
                            let value = cmds.pop_front().and_then(get_string).ok_or("Empty value")?;
                            Ok(RedisCmd::HSet(key, sub_key, value))
                        }
                        "SUB" => Ok(RedisCmd::Sub(cmds.pop_front().and_then(get_string).ok_or("Empty key")?)),
                        "HSUB" => Ok(RedisCmd::HSub(cmds.pop_front().and_then(get_string).ok_or("Empty key")?)),
                        _ => Err("NOT_SUPPORTED".to_string()),
                    }
                } else {
                    Err("NOT_WORKING".to_string())
                }
            }
        }
    }
}
