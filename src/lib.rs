#![doc = include_str!("../README.md")]

use bytes::Bytes;
// use futures::future::BoxFuture;
use http::{Request, Response, header};
use http_body_util::{BodyExt, Full, combinators::UnsyncBoxBody};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower::{Layer, Service};
use tracing::{debug, error};

#[cfg(feature = "standard")]
pub use minify_html::Cfg;

#[cfg(feature = "onepass")]
pub use minify_html_onepass::Cfg as OnePassCfg;

#[cfg(not(any(feature = "standard", feature = "onepass")))]
compile_error!("Either feature 'standard' or 'onepass' (or both) must be enabled");

#[derive(Clone, Copy, Debug)]
pub enum Backend {
    #[cfg(feature = "standard")]
    Standard,
    #[cfg(feature = "onepass")]
    Onepass,
}

impl Default for Backend {
    fn default() -> Self {
        #[cfg(feature = "standard")]
        return Backend::Standard;
        #[cfg(all(feature = "onepass", not(feature = "standard")))]
        return Backend::Onepass;
    }
}

#[derive(Clone)]
pub struct MinifyHtmlLayer {
    backend: Backend,
    #[cfg(feature = "standard")]
    standard_config: minify_html::Cfg,
    #[cfg(feature = "onepass")]
    // minify_html_onepass::Cfg is not Clone, so wrap in Arc. See https://github.com/wilsonzlin/minify-html/pull/267.
    onepass_config: std::sync::Arc<minify_html_onepass::Cfg>,
}

impl MinifyHtmlLayer {
    pub fn builder() -> MinifyHtmlLayerBuilder {
        MinifyHtmlLayerBuilder::default()
    }

    #[cfg(feature = "standard")]
    pub fn new(config: minify_html::Cfg) -> Self {
        Self::builder().standard_config(config).build()
    }
}

#[derive(Default)]
pub struct MinifyHtmlLayerBuilder {
    backend: Backend,
    #[cfg(feature = "standard")]
    standard_config: minify_html::Cfg,
    #[cfg(feature = "onepass")]
    onepass_config: minify_html_onepass::Cfg,
}

impl MinifyHtmlLayerBuilder {
    pub fn backend(mut self, backend: Backend) -> Self {
        self.backend = backend;
        self
    }

    #[cfg(feature = "standard")]
    pub fn standard_config(mut self, config: minify_html::Cfg) -> Self {
        self.standard_config = config;
        self
    }

    #[cfg(feature = "onepass")]
    pub fn onepass_config(mut self, config: minify_html_onepass::Cfg) -> Self {
        self.onepass_config = config;
        self
    }

    pub fn build(self) -> MinifyHtmlLayer {
        MinifyHtmlLayer {
            backend: self.backend,
            #[cfg(feature = "standard")]
            standard_config: self.standard_config,
            #[cfg(feature = "onepass")]
            onepass_config: std::sync::Arc::new(self.onepass_config),
        }
    }
}

impl<S> Layer<S> for MinifyHtmlLayer {
    type Service = MinifyHtml<S>;

    fn layer(&self, inner: S) -> Self::Service {
        MinifyHtml {
            inner,
            backend: self.backend,
            #[cfg(feature = "standard")]
            standard_config: self.standard_config.clone(),
            #[cfg(feature = "onepass")]
            onepass_config: self.onepass_config.clone(),
        }
    }
}

#[derive(Clone)]
pub struct MinifyHtml<S> {
    inner: S,
    backend: Backend,
    #[cfg(feature = "standard")]
    standard_config: minify_html::Cfg,
    #[cfg(feature = "onepass")]
    onepass_config: std::sync::Arc<minify_html_onepass::Cfg>,
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
        let backend = self.backend;
        #[cfg(feature = "standard")]
        let standard_config = self.standard_config.clone();
        #[cfg(feature = "onepass")]
        let onepass_config = self.onepass_config.clone();

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
                    return Ok(error_500_response());
                }
            };

            let minified = match backend {
                #[cfg(feature = "standard")]
                Backend::Standard => minify_html::minify(&bytes, &standard_config),

                #[cfg(feature = "onepass")]
                Backend::Onepass => {
                    let mut vec = bytes.to_vec();
                    match minify_html_onepass::in_place(&mut vec, &onepass_config) {
                        Ok(len) => {
                            vec.truncate(len);
                            vec
                        }
                        Err(_) => return Ok(error_500_response()),
                    }
                }
            };

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

fn error_500_response() -> Response<UnsyncBoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>>
{
    Response::builder()
        .status(500)
        .body(
            Full::from("Internal Server Error")
                .map_err(|e| e.into())
                .boxed_unsync(),
        )
        .unwrap()
}
