//! Lower-level types and re-exports.
//!
//! Most users will not have to interact with the types in this module, but it is useful for those
//! writing extractors, middleware and libraries, or interacting with the service API directly.

pub use crate::config::{AppConfig, AppService};
#[doc(hidden)]
pub use crate::handler::Handler;
pub use crate::info::{ConnectionInfo, PeerAddr};
pub use crate::rmap::ResourceMap;
pub use crate::service::{HttpServiceFactory, ServiceRequest, ServiceResponse, WebService};

pub use crate::types::form::UrlEncoded;
pub use crate::types::json::JsonBody;
pub use crate::types::readlines::Readlines;

pub use actix_http::body::{AnyBody, Body, BodySize, MessageBody, ResponseBody, SizedStream};

#[cfg(feature = "__compress")]
pub use actix_http::encoding::Decoder as Decompress;
pub use actix_http::{
    CloneableExtensions, Extensions, Payload, PayloadStream, RequestHead, ResponseHead,
};
pub use actix_router::{Path, ResourceDef, ResourcePath, Url};
pub use actix_server::Server;
pub use actix_service::{
    always_ready, fn_factory, fn_service, forward_ready, Service, ServiceFactory, Transform,
};

use crate::http::header::ContentEncoding;
use actix_http::{Response, ResponseBuilder};

pub(crate) fn insert_leading_slash(mut patterns: Vec<String>) -> Vec<String> {
    for path in &mut patterns {
        if !path.is_empty() && !path.starts_with('/') {
            path.insert(0, '/');
        };
    }

    patterns
}
struct Enc(ContentEncoding);

/// Helper trait that allows to set specific encoding for response.
pub trait BodyEncoding {
    /// Get content encoding
    fn get_encoding(&self) -> Option<ContentEncoding>;

    /// Set content encoding
    ///
    /// Must be used with [`crate::middleware::Compress`] to take effect.
    fn encoding(&mut self, encoding: ContentEncoding) -> &mut Self;
}

impl BodyEncoding for ResponseBuilder {
    fn get_encoding(&self) -> Option<ContentEncoding> {
        self.extensions().get::<Enc>().map(|enc| enc.0)
    }

    fn encoding(&mut self, encoding: ContentEncoding) -> &mut Self {
        self.extensions_mut().insert(Enc(encoding));
        self
    }
}

impl<B> BodyEncoding for Response<B> {
    fn get_encoding(&self) -> Option<ContentEncoding> {
        self.extensions().get::<Enc>().map(|enc| enc.0)
    }

    fn encoding(&mut self, encoding: ContentEncoding) -> &mut Self {
        self.extensions_mut().insert(Enc(encoding));
        self
    }
}

impl BodyEncoding for crate::HttpResponseBuilder {
    fn get_encoding(&self) -> Option<ContentEncoding> {
        self.extensions().get::<Enc>().map(|enc| enc.0)
    }

    fn encoding(&mut self, encoding: ContentEncoding) -> &mut Self {
        self.extensions_mut().insert(Enc(encoding));
        self
    }
}

impl<B> BodyEncoding for crate::HttpResponse<B> {
    fn get_encoding(&self) -> Option<ContentEncoding> {
        self.extensions().get::<Enc>().map(|enc| enc.0)
    }

    fn encoding(&mut self, encoding: ContentEncoding) -> &mut Self {
        self.extensions_mut().insert(Enc(encoding));
        self
    }
}
