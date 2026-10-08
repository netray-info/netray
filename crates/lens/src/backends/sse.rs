use futures::StreamExt;
use serde_json::Value;

/// Drain an SSE byte-stream from a reqwest Response into a Vec of JSON events.
///
/// Each event is returned as `{"type": "<event-name>", "data": <parsed-json>}`.
/// Reading stops when an event matching `terminal_event` is dispatched, or when
/// the stream ends. Returns Err on a chunk read error or UTF-8 decode failure.
pub async fn collect(resp: reqwest::Response, terminal_event: &str) -> Result<Vec<Value>, String> {
    drain(resp, terminal_event, false).await
}

/// Like [`collect`], for streams that send `data:` lines only.
///
/// The event type is the `type` field of the JSON payload (an `event:` line, if
/// present, is ignored). Reading stops at the first event whose type equals
/// `terminal_type`, or when the stream ends.
pub async fn collect_until_type(
    resp: reqwest::Response,
    terminal_type: &str,
) -> Result<Vec<Value>, String> {
    drain(resp, terminal_type, true).await
}

async fn drain(
    resp: reqwest::Response,
    terminal: &str,
    type_from_data: bool,
) -> Result<Vec<Value>, String> {
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let mut events: Vec<Value> = Vec::new();
    let mut cur_type = String::new();
    let mut cur_data = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        buf.push_str(&String::from_utf8_lossy(&chunk));

        loop {
            match buf.find('\n') {
                None => break,
                Some(pos) => {
                    let line = buf[..pos].trim_end_matches('\r').to_string();
                    buf = buf[pos + 1..].to_string();

                    if line.is_empty() {
                        // Blank line = dispatch current event.
                        if !cur_data.is_empty()
                            && let Ok(data) = serde_json::from_str::<Value>(&cur_data)
                        {
                            if type_from_data {
                                cur_type = data
                                    .get("type")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default()
                                    .to_string();
                            }
                            let is_terminal = cur_type == terminal;
                            events.push(serde_json::json!({
                                "type": cur_type,
                                "data": data,
                            }));
                            if is_terminal {
                                return Ok(events);
                            }
                        }
                        cur_type.clear();
                        cur_data.clear();
                    } else if let Some(rest) = line.strip_prefix("event: ") {
                        cur_type = rest.to_string();
                    } else if let Some(rest) = line.strip_prefix("data: ") {
                        cur_data = rest.to_string();
                    }
                }
            }
        }
    }

    Ok(events)
}
