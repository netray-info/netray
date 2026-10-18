//! `/docs`: the Scalar page over `/api/v1/openapi.json`.
use axum::response::Html;
use utoipa_scalar::Scalar;

const HTML: &str = r#"<!doctype html>
<html>
<head>
<meta charset="utf-8">
<title>netray API</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
</head>
<body>
<script id="api-reference" data-url="/api/v1/openapi.json"></script>
<script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
</body>
</html>
"#;

pub(super) async fn page() -> Html<String> {
    Html(
        Scalar::new(serde_json::Value::Null)
            .custom_html(HTML)
            .to_html(),
    )
}
