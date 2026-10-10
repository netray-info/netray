use futures::StreamExt;
use serde_json::Value;

/// Drain an SSE byte-stream from a reqwest Response into a Vec of JSON events.
///
/// Each event is returned as `{"type": "<event-name>", "data": <parsed-json>}`.
/// Reading stops when an event matching `terminal_event` is dispatched. Returns Err
/// when the stream ends without it, on a chunk read error, when a complete line is
/// not valid UTF-8, or when an event's data is not JSON.
pub async fn collect(resp: reqwest::Response, terminal: &str) -> Result<Vec<Value>, String> {
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    let mut events: Vec<Value> = Vec::new();
    let mut cur_type = String::new();
    let mut cur_data = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        buf.extend_from_slice(&chunk);

        loop {
            match buf.iter().position(|&b| b == b'\n') {
                None => break,
                Some(pos) => {
                    let raw: Vec<u8> = buf.drain(..=pos).collect();
                    let line = std::str::from_utf8(&raw[..pos])
                        .map_err(|e| format!("invalid UTF-8 in SSE stream: {e}"))?
                        .trim_end_matches('\r')
                        .to_string();

                    if line.is_empty() {
                        // Blank line = dispatch current event.
                        if !cur_data.is_empty() {
                            let data = serde_json::from_str::<Value>(&cur_data)
                                .map_err(|e| format!("invalid JSON in SSE event: {e}"))?;
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
                        if !cur_data.is_empty() {
                            cur_data.push('\n');
                        }
                        cur_data.push_str(rest);
                    }
                }
            }
        }
    }

    Err(format!("stream ended without {terminal}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resp(body: &str) -> reqwest::Response {
        reqwest::Response::from(axum::http::Response::new(body.to_string()))
    }

    #[tokio::test]
    async fn c3_stream_without_terminal_event_is_an_error() {
        let r = collect(resp("event: batch\ndata: {\"a\":1}\n\n"), "done").await;
        assert!(r.is_err(), "truncated stream must be Err, got {r:?}");
    }

    #[tokio::test]
    async fn c4_stream_with_terminal_event_returns_all_events() {
        let body = "event: batch\ndata: {\"a\":1}\n\nevent: done\ndata: {}\n\n";
        let ev = collect(resp(body), "done").await.expect("ok");
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0]["type"], "batch");
        assert_eq!(ev[0]["data"], serde_json::json!({"a":1}));
        assert_eq!(ev[1]["type"], "done");
    }

    #[tokio::test]
    async fn c5_multiline_data_is_joined_with_newline() {
        let body = "event: batch\ndata: {\"a\":\ndata: 1}\n\nevent: done\ndata: {}\n\n";
        let ev = collect(resp(body), "done").await.expect("ok");
        assert_eq!(ev[0]["data"], serde_json::json!({"a":1}));
        // The separator is a newline, not nothing: `[1` + `0]` would read as [10] without
        // one, and is two values (invalid) with it.
        let merged = "event: batch\ndata: [1\ndata: 0]\n\nevent: done\ndata: {}\n\n";
        assert!(collect(resp(merged), "done").await.is_err());
    }

    #[tokio::test]
    async fn c6_unparseable_data_is_an_error() {
        let body = "event: batch\ndata: not-json\n\nevent: done\ndata: {}\n\n";
        let r = collect(resp(body), "done").await;
        assert!(r.is_err(), "bad JSON must be Err, got {r:?}");
    }
}
