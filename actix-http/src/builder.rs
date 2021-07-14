use std::{error::Error as StdError, fmt, marker::PhantomData, net, rc::Rc};

use actix_codec::Framed;
use actix_service::{IntoServiceFactory, Service, ServiceFactory};

use crate::{
    body::{AnyBody, MessageBody},
    config::{KeepAlive, ServiceConfig},
    extensions::CloneableExtensions,
    h1::{self, ExpectHandler, H1Service, UpgradeHandler},
    h2::H2Service,
    service::HttpService,
    ConnectCallback, Request, Response,
};

/// A HTTP service builder
///
/// This type can be used to construct an instance of [`HttpService`] through a
/// builder-like pattern.
pub struct HttpServiceBuilder<T, S, X = ExpectHandler, U = UpgradeHandler> {
    keep_alive: KeepAlive,
    client_timeout: u64,
    client_disconnect: u64,
    secure: bool,
    local_addr: Option<net::SocketAddr>,
    expect: X,
    upgrade: Option<U>,
    on_connect_ext: Option<Rc<ConnectCallback<T>>>,
    _phantom: PhantomData<S>,
}

impl<T, S> HttpServiceBuilder<T, S, ExpectHandler, UpgradeHandler>
where
    S: ServiceFactory<Request, Config = ()>,
    S::Error: Into<Response<AnyBody>> + 'static,
    S::InitError: fmt::Debug,
    <S::Service as Service<Request>>::Future: 'static,
{
    /// Create instance of `ServiceConfigBuilder`
    pub fn new() -> Self {
        HttpServiceBuilder {
            keep_alive: KeepAlive::Timeout(5),
            client_timeout: 5000,
            client_disconnect: 0,
            secure: false,
            local_addr: None,
            expect: ExpectHandler,
            upgrade: None,
            on_connect_ext: None,
            _phantom: PhantomData,
        }
    }
}

impl<T, S, X, U> HttpServiceBuilder<T, S, X, U>
where
    S: ServiceFactory<Request, Config = ()>,
    S::Error: Into<Response<AnyBody>> + 'static,
    S::InitError: fmt::Debug,
    <S::Service as Service<Request>>::Future: 'static,
    X: ServiceFactory<Request, Config = (), Response = Request>,
    X::Error: Into<Response<AnyBody>>,
    X::InitError: fmt::Debug,
    U: ServiceFactory<(Request, Framed<T, h1::Codec>), Config = (), Response = ()>,
    U::Error: fmt::Display,
    U::InitError: fmt::Debug,
{
    /// Set server keep-alive setting.
    ///
    /// By default keep alive is set to a 5 seconds.
    pub fn keep_alive<W: Into<KeepAlive>>(mut self, val: W) -> Self {
        self.keep_alive = val.into();
        self
    }

    /// Set connection secure state
    pub fn secure(mut self) -> Self {
        self.secure = true;
        self
    }

    /// Set the local address that this service is bound to.
    pub fn local_addr(mut self, addr: net::SocketAddr) -> Self {
        self.local_addr = Some(addr);
        self
    }

    /// Set server client timeout in milliseconds for first request.
    ///
    /// Defines a timeout for reading client request header. If a client does not transmit
    /// the entire set headers within this time, the request is terminated with
    /// the 408 (Request Time-out) error.
    ///
    /// To disable timeout set value to 0.
    ///
    /// By default client timeout is set to 5000 milliseconds.
    pub fn client_timeout(mut self, val: u64) -> Self {
        self.client_timeout = val;
        self
    }

    /// Set server connection disconnect timeout in milliseconds.
    ///
    /// Defines a timeout for disconnect connection. If a disconnect procedure does not complete
    /// within this time, the request get dropped. This timeout affects secure connections.
    ///
    /// To disable timeout set value to 0.
    ///
    /// By default disconnect timeout is set to 0.
    pub fn client_disconnect(mut self, val: u64) -> Self {
        self.client_disconnect = val;
        self
    }

    /// Provide service for `EXPECT: 100-Continue` support.
    ///
    /// Service get called with request that contains `EXPECT` header.
    /// Service must return request in case of success, in that case
    /// request will be forwarded to main service.
    pub fn expect<F, X1>(self, expect: F) -> HttpServiceBuilder<T, S, X1, U>
    where
        F: IntoServiceFactory<X1, Request>,
        X1: ServiceFactory<Request, Config = (), Response = Request>,
        X1::Error: Into<Response<AnyBody>>,
        X1::InitError: fmt::Debug,
    {
        HttpServiceBuilder {
            keep_alive: self.keep_alive,
            client_timeout: self.client_timeout,
            client_disconnect: self.client_disconnect,
            secure: self.secure,
            local_addr: self.local_addr,
            expect: expect.into_factory(),
            upgrade: self.upgrade,
            on_connect_ext: self.on_connect_ext,
            _phantom: PhantomData,
        }
    }

    /// Provide service for custom `Connection: UPGRADE` support.
    ///
    /// If service is provided then normal requests handling get halted
    /// and this service get called with original request and framed object.
    pub fn upgrade<F, U1>(self, upgrade: F) -> HttpServiceBuilder<T, S, X, U1>
    where
        F: IntoServiceFactory<U1, (Request, Framed<T, h1::Codec>)>,
        U1: ServiceFactory<(Request, Framed<T, h1::Codec>), Config = (), Response = ()>,
        U1::Error: fmt::Display,
        U1::InitError: fmt::Debug,
    {
        HttpServiceBuilder {
            keep_alive: self.keep_alive,
            client_timeout: self.client_timeout,
            client_disconnect: self.client_disconnect,
            secure: self.secure,
            local_addr: self.local_addr,
            expect: self.expect,
            upgrade: Some(upgrade.into_factory()),
            on_connect_ext: self.on_connect_ext,
            _phantom: PhantomData,
        }
    }

    /// Sets the callback to be run on connection establishment.
    ///
    /// Has mutable access to a data container that will be merged into request extensions.
    /// This enables transport layer data (like client certificates) to be accessed in middleware
    /// and handlers.
    pub fn on_connect_ext<F>(mut self, f: F) -> Self
    where
        F: Fn(&T, &mut CloneableExtensions) + 'static,
    {
        self.on_connect_ext = Some(Rc::new(f));
        self
    }

    /// Finish service configuration and create a HTTP Service for HTTP/1 protocol.
    pub fn h1<F, B>(self, service: F) -> H1Service<T, S, B, X, U>
    where
        B: MessageBody,
        F: IntoServiceFactory<S, Request>,
        S::Error: Into<Response<AnyBody>>,
        S::InitError: fmt::Debug,
        S::Response: Into<Response<B>>,
    {
        let cfg = ServiceConfig::new(
            self.keep_alive,
            self.client_timeout,
            self.client_disconnect,
            self.secure,
            self.local_addr,
        );

        H1Service::with_config(cfg, service.into_factory())
            .expect(self.expect)
            .upgrade(self.upgrade)
            .on_connect_ext(self.on_connect_ext)
    }

    /// Finish service configuration and create a HTTP service for HTTP/2 protocol.
    pub fn h2<F, B>(self, service: F) -> H2Service<T, S, B>
    where
        F: IntoServiceFactory<S, Request>,
        S::Error: Into<Response<AnyBody>> + 'static,
        S::InitError: fmt::Debug,
        S::Response: Into<Response<B>> + 'static,

        B: MessageBody + 'static,
        B::Error: Into<Box<dyn StdError>>,
    {
        let cfg = ServiceConfig::new(
            self.keep_alive,
            self.client_timeout,
            self.client_disconnect,
            self.secure,
            self.local_addr,
        );

        H2Service::with_config(cfg, service.into_factory())
            .on_connect_ext(self.on_connect_ext)
    }

    /// Finish service configuration and create `HttpService` instance.
    pub fn finish<F, B>(self, service: F) -> HttpService<T, S, B, X, U>
    where
        F: IntoServiceFactory<S, Request>,
        S::Error: Into<Response<AnyBody>> + 'static,
        S::InitError: fmt::Debug,
        S::Response: Into<Response<B>> + 'static,

        B: MessageBody + 'static,
        B::Error: Into<Box<dyn StdError>>,
    {
        let cfg = ServiceConfig::new(
            self.keep_alive,
            self.client_timeout,
            self.client_disconnect,
            self.secure,
            self.local_addr,
        );

        HttpService::with_config(cfg, service.into_factory())
            .expect(self.expect)
            .upgrade(self.upgrade)
            .on_connect_ext(self.on_connect_ext)
    }
}
