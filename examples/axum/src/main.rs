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
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn handler() -> Html<&'static str> {
    Html(
        r#"
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8" 
                >
                <title>    Hello    World    </title>


            </head>
            <body>
                <h1>    Hello    World    </h1>
                
            </body>
        </html>
        "#,
    )
}
