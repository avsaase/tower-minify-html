# tower-minify-html

[![Crates.io](https://img.shields.io/crates/v/tower-minify-html.svg)](https://crates.io/crates/tower-minify-html)
[![Docs.rs](https://docs.rs/tower-minify-html/badge.svg)](https://docs.rs/tower-minify-html)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/alexanderepstein/tower-minify-html#license)

A Tower layer for minifying HTML responses using `minify-html`.

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
tower-minify-html = "0.1.0"
```

## Example

```rust
use axum::{Router, response::Html, routing::get};
use minify_html::Cfg;
use tower_minify_html::MinifyHtmlLayer;

#[tokio::main]
async fn main() {
    let mut cfg = Cfg::new();
    cfg.keep_closing_tags = true;
    cfg.keep_html_and_head_opening_tags = true;
    cfg.keep_comments = false;

    let app = Router::new()
        .route("/", get(handler))
        .layer(MinifyHtmlLayer::new(cfg));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handler() -> Html<&'static str> {
    Html(
        r#"
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <title>    Hello    World    </title>
            </head>
            <body>
                <h1>    Hello    World    </h1>
            </body>
        </html>
        "#,
    )
}
```

## License

MIT OR Apache-2.0
