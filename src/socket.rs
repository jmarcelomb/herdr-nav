use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

/// herdr's socket server closes the connection after each response
/// (verified empirically on 0.7.5: a second request on the same
/// connection gets a broken pipe), so this client opens a fresh Unix
/// socket connection per call instead of holding one open. That is
/// still the whole point of this binary: a local Unix socket
/// connect+round-trip costs a fraction of a millisecond, orders of
/// magnitude cheaper than spawning a new `herdr <subcommand>` process
/// (fork+exec+dynamic-link) for every step of a navigation.
pub struct Client {
    path: String,
    next_id: u64,
}

impl Client {
    pub fn new(path: String) -> Self {
        Self { path, next_id: 0 }
    }

    /// Send one newline-delimited JSON request over a new connection
    /// and block for its response.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id.to_string();
        let request = json!({ "id": id, "method": method, "params": params });

        let mut stream = UnixStream::connect(&self.path)
            .map_err(|e| format!("connect to herdr socket failed: {e}"))?;

        let mut line = serde_json::to_string(&request).map_err(|e| e.to_string())?;
        line.push('\n');
        stream
            .write_all(line.as_bytes())
            .map_err(|e| format!("write to herdr socket failed: {e}"))?;
        stream
            .flush()
            .map_err(|e| format!("flush herdr socket failed: {e}"))?;

        let mut reader = BufReader::new(stream);
        let mut response_line = String::new();
        reader
            .read_line(&mut response_line)
            .map_err(|e| format!("read from herdr socket failed: {e}"))?;
        if response_line.is_empty() {
            return Err(format!(
                "herdr socket closed while waiting for {method} response"
            ));
        }

        let response: Value = serde_json::from_str(&response_line)
            .map_err(|e| format!("invalid {method} response: {e}"))?;

        if let Some(error) = response.get("error") {
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            return Err(format!("{method} failed: {message}"));
        }

        response
            .get("result")
            .cloned()
            .ok_or_else(|| format!("{method}: response had no result or error"))
    }
}
