use futures::StreamExt;
use serde_json::Value;

/// Drain an SSE byte-stream from a reqwest Response into a Vec of JSON events.
///
/// Each event is returned as `{"type": "<event-name>", "data": <parsed-json>}`.
/// Reading stops when an event matching `terminal_event` is dispatched, or when
/// the stream ends. Returns Err on a chunk read error or UTF-8 decode failure.
pub async fn collect(resp: reqwest::Response, terminal_event: &str) -> Result<Vec<Value>, String> {
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
                            let terminal = cur_type == terminal_event;
                            events.push(serde_json::json!({
                                "type": cur_type,
                                "data": data,
                            }));
                            if terminal {
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
