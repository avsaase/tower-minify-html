#![doc = include_str!("../README.md")]

use bytes::Bytes;
// use futures::future::BoxFuture;
use http::{Request, Response, header};
use http_body_util::{BodyExt, Full, combinators::UnsyncBoxBody};
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tower::{Layer, Service};
use tracing::{debug, error};

pub use minify_html::Cfg;

#[derive(Clone)]
pub struct MinifyHtmlLayer {
    config: Cfg,
}

impl MinifyHtmlLayer {
    pub fn new(config: Cfg) -> Self {
        Self { config }
    }
}

impl<S> Layer<S> for MinifyHtmlLayer {
    type Service = MinifyHtml<S>;

    fn layer(&self, inner: S) -> Self::Service {
        MinifyHtml {
            inner,
            config: self.config.clone(),
        }
    }
}

#[derive(Clone)]
pub struct MinifyHtml<S> {
    inner: S,
    config: Cfg,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for MinifyHtml<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Send + 'static,
    S::Future: Send + 'static,
    ResBody: BodyExt<Data = Bytes> + Send + 'static,
    ResBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Response = Response<UnsyncBoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>;
    type Error = S::Error;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        let response_future = self.inner.call(request);
        let config = self.config.clone();

        Box::pin(async move {
            let response = response_future.await?;

            let is_html = response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|v| v.contains("text/html"))
                .unwrap_or(false);

            if !is_html {
                return Ok(response.map(|b| b.map_err(|e| e.into()).boxed_unsync()));
            }

            let (mut parts, body) = response.into_parts();

            // Remove Content-Length as it changes
            parts.headers.remove(header::CONTENT_LENGTH);

            let bytes = match body.collect().await {
                Ok(c) => c.to_bytes(),
                Err(_e) => {
                    error!("Failed to collect response body for minification");
                    return Ok(Response::builder()
                        .status(500)
                        .body(
                            Full::from("Error processing response body")
                                .map_err(|e| e.into())
                                .boxed_unsync(),
                        )
                        .unwrap());
                }
            };

            let minified = minify_html::minify(&bytes, &config);
            debug!(
                "HTML minified: original size {} bytes, minified size {} bytes",
                bytes.len(),
                minified.len()
            );

            let new_body = Full::new(Bytes::from(minified))
                .map_err(|_e| unreachable!("Full body never errors"))
                .boxed_unsync();

            Ok(Response::from_parts(parts, new_body))
        })
    }
}
