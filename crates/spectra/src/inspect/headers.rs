use indexmap::IndexMap;
use reqwest::header::HeaderMap;

/// Dump all response headers as an ordered map (lowercase names, first value wins).
pub fn dump_headers(headers: &HeaderMap) -> IndexMap<String, String> {
    let mut map = IndexMap::new();
    for (name, value) in headers.iter() {
        let key = name.as_str().to_lowercase();
        if !map.contains_key(&key) && let Ok(v) = value.to_str() {
            map.insert(key, v.to_string());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dumps_headers_lowercase() {
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", "text/html".parse().unwrap());
        headers.insert("X-Custom", "value".parse().unwrap());

        let map = dump_headers(&headers);
        assert_eq!(map.get("content-type").unwrap(), "text/html");
        assert_eq!(map.get("x-custom").unwrap(), "value");
    }

    #[test]
    fn first_value_wins() {
        let mut headers = HeaderMap::new();
        headers.insert("x-test", "first".parse().unwrap());
        headers.append("x-test", "second".parse().unwrap());

        let map = dump_headers(&headers);
        assert_eq!(map.get("x-test").unwrap(), "first");
    }
}
